use tracing::{error, info, instrument, warn};

use app_database::entity::accounts::Platform;

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
        // One-shot startup scan: recover in-progress/delivering requests as
        // supervised per-request watchers. Per-request watches own their own
        // lifecycles now; there is no persistent snapshot loop.
        if let Err(e) = startup_scan().await {
            warn!(?e, "Startup scan exited with error; giving up");
        }
    });

    tokio::task::spawn(async {
        crate::cmd::_common::account_refresh::run_refresh_loop(
            Platform::Telegram,
            bot::helpers::account::fetch_user_fut,
            bot::helpers::account::fetch_place_fut,
        )
        .await;
    });

    bot::TelegramBot::run()
        .await
        .map_err(anyhow::Error::into_boxed_dyn_error)
}

/// Startup scan: fetch this bot's in-progress/delivering requests once and
/// submit each to the keyed supervisor as a recovery watcher. Retried through
/// the reconnect coordinator until it succeeds or the platform is shutting
/// down; a central/database failure is never treated as an empty successful
/// scan.
async fn startup_scan() -> Result<(), anyhow::Error> {
    loop {
        match RpcClient::work_request_list_mine_in_progress().await {
            Ok(snapshot) => {
                if let Some(e) = snapshot.error {
                    warn!(?e, "startup scan returned error; reconnecting");
                    reconnect_and_backoff().await;
                    continue;
                }
                for req in snapshot.requests.iter() {
                    recover_request(req).await;
                }
                info!(count = snapshot.requests.len(), "startup scan complete");
                return Ok(());
            }
            Err(e) => {
                error!(?e, "startup scan failed; reconnecting");
                reconnect_and_backoff().await;
            }
        }
    }
}

/// Reconstruct the platform delivery object from request metadata and submit
/// the request to the keyed supervisor. A delivering row starts a recovery
/// watcher that stays subscribed through lease expiry.
async fn recover_request(req: &app_peer_comms::message::v1::central::work_request::WorkRequest) {
    use crate::cmd::telegram::bot::{
        handlers::delivery::start_request_task, helpers::status_message::StatusMessage,
    };

    let request_id = req.request_id();
    let is_recovery = req.status().is_delivering();
    let status_message = match StatusMessage::from_metadata(req.metadata()) {
        Ok(x) => x,
        Err(e) => {
            error!(
                ?e,
                ?request_id,
                "failed to reconstruct status message for recovery"
            );
            return;
        }
    };
    start_request_task(request_id, status_message, is_recovery).await;
}

/// Reconnect (single-flight) with bounded backoff. Used by the startup scan
/// when central/database is unavailable.
async fn reconnect_and_backoff() {
    if let Err(e) = peering::reconnect().await {
        warn!(?e, "reconnect failed during startup scan");
    }
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}
