use std::sync::Arc;

use tracing::{debug, error, info, instrument, warn};

use super::CmdResult;
use crate::{
    cmd::telegram::config::TelegramConfig,
    peering::{self, rpc::RpcClient},
};

mod bot;
pub mod common;
pub mod config;

#[instrument(name = "telegram", skip_all)]
pub async fn run(config: TelegramConfig) -> CmdResult {
    info!("Starting command bot...");

    bot::TelegramBot::init(config.bot);

    tokio::task::spawn(async move {
        loop {
            if let Err(e) = watch_work_requests().await {
                warn!(?e, "Work requests watcher exited with error");
                if let Err(re) = peering::reconnect().await {
                    warn!(?re, "Failed to re-bootstrap irpc session; will retry");
                }
            }

            let about_two_seconds = 2000 + rand::random_range(0..=2000);
            let about_two_seconds = std::time::Duration::from_millis(about_two_seconds);

            debug!(time = ?about_two_seconds, "Work requests stream finished. Sleeping for a random amount of time...");

            tokio::time::sleep(about_two_seconds).await;
        }
    });

    bot::TelegramBot::run()
        .await
        .map_err(anyhow::Error::into_boxed_dyn_error)
}

async fn watch_work_requests() -> Result<(), anyhow::Error> {
    debug!("Starting to watch work requests");
    let mut reqs_it = match RpcClient::work_request_watch_mine_in_progress().await {
        Ok(x) => x,
        Err(e) => {
            error!(?e, "Failed to watch work requests");
            return Err(e.into());
        }
    };

    debug!("Connected to work requests watcher");

    while let Some(snapshot) = match reqs_it.recv().await {
        Ok(x) => x,
        Err(e) => {
            error!(?e, "Got error from work requests watcher");
            return Err(e.into());
        }
    } {
        for req in snapshot.requests.iter().cloned() {
            let req = Arc::new(req);
            let status_message =
                match bot::helpers::status_message::StatusMessage::from_metadata(req.metadata()) {
                    Ok(x) => x,
                    Err(e) => {
                        error!(?e, "Failed to get status message");
                        continue;
                    }
                };

            tokio::task::spawn(bot::handlers::message::process_work_request(
                req.clone(),
                status_message,
            ));
        }
    }

    Ok(())
}
