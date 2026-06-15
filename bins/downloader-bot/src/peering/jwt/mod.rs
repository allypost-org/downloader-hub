use std::{
    ops::Deref,
    sync::{Arc, OnceLock},
};

use app_peer_comms::PeeringEndpoint;
use tokio::sync::RwLock;
use tracing::{debug, error, instrument, trace};

static JWT_PAIR: OnceLock<Arc<RwLock<JwtPair>>> = OnceLock::new();

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct JwtPair(app_peer_comms::jwt::JwtPair);

impl JwtPair {
    #[must_use]
    pub const fn new(pair: app_peer_comms::jwt::JwtPair) -> Self {
        Self(pair)
    }
}

impl Deref for JwtPair {
    type Target = app_peer_comms::jwt::JwtPair;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<app_peer_comms::jwt::JwtPair> for JwtPair {
    fn from(value: app_peer_comms::jwt::JwtPair) -> Self {
        Self(value)
    }
}

impl JwtPair {
    pub fn init(pair: Self) {
        JWT_PAIR
            .set(Arc::new(RwLock::new(pair)))
            .expect("Failed to set JWT pair");
    }

    pub fn global() -> Arc<RwLock<Self>> {
        JWT_PAIR.get().expect("JWT pair not initialized").clone()
    }

    pub async fn refresh_token() -> Arc<str> {
        Self::global().read().await.refresh_token.clone()
    }

    pub async fn token() -> Arc<str> {
        Self::global().read().await.token.clone()
    }

    #[instrument(skip_all)]
    pub async fn refresh_via_refresh_token(
        base_url: &url::Url,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        trace!(target: PeeringEndpoint::trace_span_name(), "Refreshing JWT token");

        let refresh_token = Self::refresh_token().await;
        let new = fetch_by_refresh_token(base_url, refresh_token).await?;

        *Self::global().write().await = new;

        Ok(())
    }
}

pub async fn fetch_by_refresh_token(
    base_url: &url::Url,
    refresh_token: Arc<str>,
) -> Result<JwtPair, Box<dyn std::error::Error + Send + Sync>> {
    #[derive(Debug, serde::Deserialize)]
    #[serde(rename_all = "camelCase", tag = "status")]
    enum Resp {
        Ok { data: JwtPair },
        Error { error: String },
    }

    debug!(target: PeeringEndpoint::trace_span_name(), "Fetching token with refresh token");
    let token_response = reqwest::Client::new()
        .post(base_url.join("/api/v1/auth/refresh")?)
        .header("Content-Type", "application/json")
        .body(
            serde_json::to_string(&serde_json::json!({
                "refreshToken": refresh_token,
            }))
            .expect("failed to serialize string"),
        )
        .send()
        .await?
        .error_for_status()?
        .json::<Resp>()
        .await?;

    trace!(target: PeeringEndpoint::trace_span_name(), ?token_response, "Fetched token with refresh token");

    match token_response {
        Resp::Ok { data } => Ok(data),
        Resp::Error { error: err } => {
            error!(target: PeeringEndpoint::trace_span_name(), ?err, "Failed to fetch token");
            Err(err.into())
        }
    }
}
