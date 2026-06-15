use std::{collections::BTreeMap, sync::OnceLock};

use app_config::common::DatabaseConfig;
use convex::{ConvexClient, ConvexClientBuilder, FunctionResult, WebSocketState};
use futures::StreamExt;
use tokio::sync::Mutex;
use tracing::{debug, instrument, trace};

use crate::error::{DatabaseError, ResponseError};

static GLOBAL: OnceLock<Database> = OnceLock::new();

pub struct Database {
    client: Mutex<ConvexClient>,
}

impl Database {
    pub fn global() -> &'static Self {
        GLOBAL.get().expect("Global database not initialized")
    }

    pub async fn init(cfg: DatabaseConfig) -> Result<(), DatabaseError> {
        if GLOBAL.get().is_some() {
            trace!("Database already initialized");
            return Err(DatabaseError::AlreadyInitialized);
        }

        trace!("Initializing database");

        let res = GLOBAL
            .set(Self::new(cfg).await?)
            .map_err(|_| DatabaseError::AlreadyInitialized);

        if res.is_ok() {
            debug!("Database initialized");
        }

        res
    }

    #[instrument(name = "new_database_client", skip_all)]
    pub async fn new(cfg: DatabaseConfig) -> Result<Self, DatabaseError> {
        trace!(config = ?cfg, "Creating new database client");

        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let client = ConvexClientBuilder::new(&cfg.database_url)
            .with_on_state_change(tx)
            .build()
            .await
            .map_err(DatabaseError::Init)?;

        trace!("Database client created");

        let span = tracing::Span::current();
        tokio::task::spawn(async move {
            let _span = span.enter();
            debug!("Waiting for database to connect");
            while let Some(state) = rx.recv().await {
                #[allow(clippy::match_wildcard_for_single_variants)]
                match state {
                    WebSocketState::Connected => {
                        debug!("Database client connected");
                        break;
                    }
                    s => {
                        trace!(?s, "Database state changed");
                    }
                }
            }
        })
        .await
        .map_err(|e| DatabaseError::FailedToConnect(e.into()))?;

        Ok(Self {
            client: Mutex::new(client),
        })
    }

    pub async fn query<T>(
        &self,
        name: &str,
        args: BTreeMap<String, convex::Value>,
    ) -> Result<T, DatabaseError>
    where
        T: serde::de::DeserializeOwned,
    {
        let res = self
            .client
            .lock()
            .await
            .query(name, args)
            .await
            .map_err(DatabaseError::Base)?;

        Self::process_db_result(res).map_err(DatabaseError::Response)
    }

    pub async fn mutation<T>(
        &self,
        name: &str,
        args: BTreeMap<String, convex::Value>,
    ) -> Result<T, DatabaseError>
    where
        T: serde::de::DeserializeOwned,
    {
        let res = self
            .client
            .lock()
            .await
            .mutation(name, args)
            .await
            .map_err(DatabaseError::Base)?;

        Self::process_db_result(res).map_err(DatabaseError::Response)
    }

    pub async fn watch_query<T>(
        &self,
        name: &str,
        args: BTreeMap<String, convex::Value>,
    ) -> Result<impl futures::stream::Stream<Item = Result<T, ResponseError>> + use<T>, DatabaseError>
    where
        T: serde::de::DeserializeOwned,
    {
        self.client
            .lock()
            .await
            .subscribe(name, args)
            .await
            .map_err(DatabaseError::Base)
            .map(|x| x.map(Self::process_db_result))
    }
}

impl Database {
    pub fn process_db_result<T>(result: FunctionResult) -> Result<T, ResponseError>
    where
        T: serde::de::DeserializeOwned,
    {
        match result {
            FunctionResult::Value(v) => {
                let v = v.export();
                Ok(serde_json::from_value(v)?)
            }
            FunctionResult::ErrorMessage(e) => Err(ResponseError::Response(e)),
            FunctionResult::ConvexError(e) => Err(ResponseError::ConvexError(e)),
        }
    }
}
