use std::sync::Arc;

use app_peer_comms::message::v1::worker::CommunicationType;
use axum::extract::ws::{self, WebSocket};
use futures::{StreamExt, stream::SplitStream};
use tracing::{debug, trace, warn};

use crate::cmd::central::{
    broadcaster::BroadcastAudience,
    components::worker_api::{auth::ValidAuth, event_handler::socket_sender::SocketSender},
    rpc_handler::handle_rpc,
};

pub async fn handle_socket_messages(
    sender: Arc<SocketSender>,
    mut receiver: SplitStream<WebSocket>,
    auth: ValidAuth,
) {
    while let Some(msg) = receiver.next().await {
        let parsed: app_peer_comms::Message = match msg {
            Ok(ws_message) => match ws_message {
                ws::Message::Text(text) => match CommunicationType::Json.decode(text.as_bytes()) {
                    Ok(x) => x,
                    Err(e) => {
                        warn!(?text, %e, "Failed to parse incoming text message");
                        continue;
                    }
                },
                ws::Message::Binary(bin) => {
                    match CommunicationType::Postcard.decode(bin.as_ref()) {
                        Ok(x) => x,
                        Err(e) => {
                            warn!(?bin, %e, "Failed to parse incoming binary message");
                            continue;
                        }
                    }
                }
                ws::Message::Close(reason) => {
                    debug!(?reason, "Received close message");
                    break;
                }
                ws::Message::Ping(data) => {
                    if data.len() < 128 {
                        debug!(?data, "Got reasonable ping");
                        sender.pong(data).await;
                    } else {
                        debug!(len = data.len(), "Got unreasonably long ping, not ponging");
                    }
                    continue;
                }
                ws::Message::Pong(_) => {
                    continue;
                }
            },
            Err(e) => {
                debug!(?e, "Failed to receive message");
                break;
            }
        };

        trace!(msg = ?parsed, "Received message");

        let audiences = vec![
            BroadcastAudience::Authed(auth.authed_id.clone()),
            BroadcastAudience::Socket(sender.id()),
        ];

        match handle_rpc(parsed, auth.clone(), audiences).await {
            Ok(msg) => {
                trace!(?msg, "Handled rpc");
            }
            Err(e) => {
                warn!(?e, "Failed to handle rpc");
            }
        }
    }

    debug!("Socket closed");
}
