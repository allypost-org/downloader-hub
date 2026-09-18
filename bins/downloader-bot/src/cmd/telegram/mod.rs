use std::sync::Arc;

use app_config::common::ProgramPathConfig;
use app_database::entity::accounts::Platform;
use tracing::{error, info, instrument, warn};

use super::CmdResult;
use crate::{cmd::telegram::config::TelegramConfig, peering::rpc::RpcClient};

mod bot;
pub mod common;
pub mod config;

#[instrument(name = "telegram", skip_all)]
pub async fn run(
    config: TelegramConfig,
    rpc: Arc<RpcClient>,
    dependency_paths: Arc<ProgramPathConfig>,
) -> CmdResult {
    info!("Starting command bot...");

    let tg = Arc::new(bot::TelegramBot::new(config.bot, dependency_paths));

    tokio::task::spawn({
        let rpc = Arc::clone(&rpc);
        let tg = Arc::clone(&tg);
        async move {
            // One-shot startup scan: recover in-progress/delivering requests as
            // supervised per-request watchers. Per-request watches own their own
            // lifecycles now; there is no persistent snapshot loop.
            if let Err(e) = startup_scan(&rpc, &tg).await {
                warn!(?e, "Startup scan exited with error; giving up");
            }
        }
    });

    tokio::task::spawn({
        let rpc = Arc::clone(&rpc);
        let tg = Arc::clone(&tg);
        async {
            crate::cmd::_common::account_refresh::run_refresh_loop(
                rpc,
                Platform::Telegram,
                bot::helpers::account::fetch_user_fut(Arc::clone(&tg)),
                bot::helpers::account::fetch_place_fut(tg),
            )
            .await;
        }
    });

    bot::TelegramBot::run(tg, rpc)
        .await
        .map_err(anyhow::Error::into_boxed_dyn_error)
}

/// Startup scan: fetch this bot's in-progress/delivering requests once and
/// submit each to the keyed supervisor as a recovery watcher. Retried through
/// the reconnect coordinator until it succeeds or the platform is shutting
/// down; a central/database failure is never treated as an empty successful
/// scan.
async fn startup_scan(
    rpc: &Arc<RpcClient>,
    tg: &Arc<bot::TelegramBot>,
) -> Result<(), anyhow::Error> {
    loop {
        match rpc.work_request_list_mine_in_progress().await {
            Ok(snapshot) => {
                if let Some(e) = snapshot.error {
                    warn!(?e, "startup scan returned error; reconnecting");
                    reconnect_and_backoff(rpc).await;
                    continue;
                }
                for req in snapshot.requests.iter() {
                    recover_request(rpc, tg, req).await;
                }
                info!(count = snapshot.requests.len(), "startup scan complete");
                return Ok(());
            }
            Err(e) => {
                error!(?e, "startup scan failed; reconnecting");
                reconnect_and_backoff(rpc).await;
            }
        }
    }
}

/// Reconstruct the platform delivery object from request metadata and submit
/// the request to the keyed supervisor. A delivering row starts a recovery
/// watcher that stays subscribed through lease expiry.
async fn recover_request(
    rpc: &Arc<RpcClient>,
    tg: &Arc<bot::TelegramBot>,
    req: &app_peer_comms::message::v1::central::work_request::WorkRequest,
) {
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
    }
    .with_bot(Arc::clone(tg));
    start_request_task(Arc::clone(rpc), request_id, status_message, is_recovery).await;
}

/// Reconnect (single-flight) with bounded backoff. Used by the startup scan
/// when central/database is unavailable.
async fn reconnect_and_backoff(rpc: &RpcClient) {
    if let Err(e) = rpc.reconnect().await {
        warn!(?e, "reconnect failed during startup scan");
    }
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}
