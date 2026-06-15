use std::sync::Arc;

use futures::StreamExt;
use tracing::{debug, error, info, instrument, trace, warn};

use super::CmdResult;
use crate::{cmd::telegram::config::TelegramConfig, peering::rpc::RpcClient};

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

    while let Some(ws_msg) = reqs_it.next().await {
        let ws_msg = match ws_msg {
            Ok(x) => x,
            Err(e) => {
                error!(?e, "Got error from work requests watcher socket");
                return Err(e.into());
            }
        };

        let msg_bytes = match ws_msg {
            tokio_tungstenite::tungstenite::Message::Binary(x) => x,
            tokio_tungstenite::tungstenite::Message::Text(x) => x.into(),
            msg => {
                trace!(?msg, "Got unknown message type");
                continue;
            }
        };

        let work_requests = match serde_json::from_slice::<
            Arc<[Arc<app_peer_comms::message::v1::central::work_request::WorkRequest>]>,
        >(&msg_bytes)
        {
            Ok(x) => x,
            Err(e) => {
                warn!(?e, "Failed to parse work request");
                continue;
            }
        };

        for req in work_requests.iter() {
            let status_message =
                match bot::helpers::status_message::StatusMessage::from_metadata(&req.metadata) {
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
