use std::sync::Arc;

use app_peer_comms::message::v1::central::work_request::WorkRequest;
use tracing::{error, info, trace, warn};

use crate::{
    cmd::discord::bot::{
        discord_bot::DiscordBot, handlers::delivery::start_request_task,
        helpers::status_message::StatusMessage,
    },
    peering::rpc::RpcClient,
};

/// Startup scan: fetch this bot's in-progress/delivering requests once and
/// submit each to the keyed supervisor as a recovery watcher. Retried through
/// the reconnect coordinator until it succeeds or the platform is shutting
/// down; a central/database failure is never treated as an empty successful
/// scan.
pub async fn startup_scan(
    rpc: &Arc<RpcClient>,
    bot: &Arc<DiscordBot>,
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
                    recover_request(rpc, bot, req).await;
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
async fn recover_request(rpc: &Arc<RpcClient>, bot: &Arc<DiscordBot>, req: &WorkRequest) {
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
    .with_bot(Arc::clone(bot));
    trace!(
        ?request_id,
        is_recovery, "submitting recovered request to supervisor"
    );
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
