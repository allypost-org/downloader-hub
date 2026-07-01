use app_peer_comms::message::v1::V1Message;

pub(super) use super::{RpcError, RpcReturn};
use crate::cmd::central::{auth::ValidAuth, broadcaster::BroadcastAudience};

mod bot;
mod worker;

pub async fn handle_v1_rpc(
    msg: V1Message,
    auth: ValidAuth,
    audiences: Vec<BroadcastAudience>,
) -> RpcReturn {
    match msg {
        V1Message::Bot(x) => bot::handle_bot_rpc(x, auth, audiences).await,
        V1Message::Worker(x) => worker::handle_worker_rpc(x, auth, audiences).await,
        V1Message::Central(_) => Err("Central cannot handle central messages".into()),
    }
}
