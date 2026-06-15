use std::sync::{Arc, LazyLock};

use app_database::Database;
use futures::StreamExt;
use tokio::{sync::RwLock, task::JoinSet};
use tracing::{debug, info, trace, warn};

use crate::cmd::central::components::_ipc::IpcMessage;

pub static LATEST_WORKER_REQUESTS: LazyLock<
    Arc<RwLock<Arc<[app_database::api::requests::RequestInfoResponse]>>>,
> = LazyLock::new(|| Arc::new(RwLock::new(Arc::new([]))));

pub async fn run(config: app_config::common::DatabaseConfig) -> super::ComponentResult {
    _ = app_database::Database::init(config).await;

    info!("Component ready");

    IpcMessage::DatabaseReady.send()?;

    let mut js: JoinSet<Result<(), super::ComponentError>> = tokio::task::JoinSet::new();

    js.spawn(async move {
        trace!("Starting task request watcher");
        let mut its = Database::global().requests_watch_all_available().await?;
        while let Some(req) = its.next().await {
            match req {
                Ok(req) => {
                    *LATEST_WORKER_REQUESTS.write().await = req.clone();

                    if req.is_empty() {
                        continue;
                    }

                    debug!(?req, "Received request from db");

                    IpcMessage::WorkerRequests(req).send()?;
                }

                Err(e) => {
                    warn!(?e, "Error reading request from database");
                }
            }
        }

        Ok(())
    });

    _ = js.join_all().await;

    Ok(())
}
