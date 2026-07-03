use app_database::Database;
use futures::StreamExt;
use tokio::task::JoinSet;
use tracing::{debug, info, trace, warn};

pub async fn run(config: app_config::common::DatabaseConfig) -> super::ComponentResult {
    _ = app_database::Database::init(config).await;

    info!("Component ready");

    let mut js: JoinSet<Result<(), super::ComponentError>> = tokio::task::JoinSet::new();

    let distributor_handle = super::rpc::take_initial_distributor_handle()
        .unwrap_or_else(super::rpc::respawn_distributor);
    js.spawn(async move {
        distributor_handle
            .await
            .map_err(|e| -> super::ComponentError {
                format!("WorkDistributor task ended: {e}").into()
            })
    });

    js.spawn(async move {
        trace!("Starting available-work watcher");
        let mut its = Database::global().requests_watch_all_available().await?;
        while let Some(emission) = its.next().await {
            match emission {
                Ok(req) => {
                    debug!(count = req.len(), "Received available work from db");
                    super::rpc::distributor().set_available(req).await;
                }
                Err(e) => warn!(?e, "Error reading available work from database"),
            }
        }
        Ok(())
    });

    js.spawn(async move {
        trace!("Starting authed revocation watcher");
        super::rpc::run_revocation_watcher().await
    });

    if let Some(res) = js.join_next().await {
        match res {
            Ok(Ok(())) => warn!("Database component task exited unexpectedly; restarting"),
            Ok(Err(e)) => warn!(?e, "Database component task failed; restarting"),
            Err(e) => warn!(?e, "Database component task panicked; restarting"),
        }
    }

    Ok(())
}
