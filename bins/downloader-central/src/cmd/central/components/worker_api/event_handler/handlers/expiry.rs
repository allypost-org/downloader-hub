use std::sync::Arc;

use axum::extract::ws;
use tracing::{debug, trace};

use crate::cmd::central::components::worker_api::event_handler::socket_sender::SocketSender;

pub async fn wait_for_expire(sender: Arc<SocketSender>, until_expiry: std::time::Duration) {
    trace!(?until_expiry, "Waiting for token to expire");
    tokio::time::sleep(until_expiry).await;

    debug!("Token expired, closing connection");
    sender
        .close(Some(ws::CloseFrame {
            code: ws::close_code::POLICY,
            reason: "Token expired".into(),
        }))
        .await;
}
