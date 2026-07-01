use std::sync::Arc;

use app_peer_comms::message::v1::central::CentralMessage;
use futures::{StreamExt, stream::SplitStream};
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::{Instrument, debug, error, trace, warn};

use super::socket_sender::SocketSender;

pub mod handlers;

pub async fn handle_socket(
    mut receiver: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    sender: Arc<SocketSender>,
) -> crate::cmd::CmdResult {
    while let Some(msg) = receiver.next().await {
        let msg = match msg {
            Ok(x) => x,
            Err(e) => {
                warn!(?e, "Error receiving message");
                return Err(e.into());
            }
        };

        let msg = match msg {
            tungstenite::Message::Text(text) => {
                let text = text.as_str();
                match app_peer_comms::Message::decode_string(text) {
                    Ok(msg) => msg,
                    Err(e) => {
                        warn!(?e, ?text, "Failed to parse text message");
                        continue;
                    }
                }
            }
            tungstenite::Message::Binary(bin) => {
                let bin = bin.as_ref();
                match app_peer_comms::Message::decode_bytes(bin) {
                    Ok(msg) => msg,
                    Err(e) => {
                        warn!(?e, ?bin, "Failed to parse binary message");
                        continue;
                    }
                }
            }
            tungstenite::Message::Close(reason) => {
                debug!(?reason, "Connection closed by remote");
                break;
            }
            tungstenite::Message::Ping(data) => {
                sender.pong(data).await;
                continue;
            }
            tungstenite::Message::Pong(_) => {
                continue;
            }
            tungstenite::Message::Frame(_) => {
                warn!("Received frame???");
                continue;
            }
        };

        let app_peer_comms::Message::V1(app_peer_comms::message::v1::V1Message::Central(msg)) = msg
        else {
            warn!("Received non-central message");
            continue;
        };

        if let Err(e) = tokio::task::spawn(handle_message(msg).in_current_span()).await {
            error!(?e, "Handle message panicked");
        }
    }

    Ok(())
}

pub async fn handle_message(msg: CentralMessage) {
    trace!(?msg, "Received message");

    match msg {
        CentralMessage::WorkRequest(request) => {
            _ = handlers::work_request::handle_work_request(&request).await;
        }
        CentralMessage::WorkRequests(requests) => {
            for request in &*requests {
                let handled = matches!(
                    handlers::work_request::handle_work_request(request).await,
                    Ok(true)
                );

                if handled {
                    break;
                }
            }
        }
        CentralMessage::WorkRequestsTakeResponse(resp) => {
            handlers::work_request::take::handle_take_work_request(resp).await;
        }
        CentralMessage::WorkRequestFreed(res) => {
            debug!(?res, "Work request freed");

            handlers::work_request::RECENTLY_HANDLED
                .invalidate(&res.request_id().to_string())
                .await;
        }
        CentralMessage::WorkRequestFinishResponse(res) => {
            debug!(?res, "Work request finished");
        }
        CentralMessage::WorkRequestAddErrorsResult(res) => {
            debug!(?res, "Work request add errors result");
        }
        CentralMessage::WorkRequestCreateResponse(res) => {
            debug!(?res, "Work request create response");
        }
        CentralMessage::WorkRequestUpdateStatusMessageResult(res) => {
            debug!(?res, "Work request update status message result");
        }
        CentralMessage::WorkRequestMoveToWaitingForRequesterResult(res) => {
            debug!(?res, "Work request move to waiting for requester result");
        }
        CentralMessage::WorkRequestFailResult(res) => {
            debug!(?res, "Work request fail result");
        }
        CentralMessage::WorkRequestFailed(res) => {
            debug!(?res, "Work request failed");
        }
        CentralMessage::AcceptAuthentication { .. }
        | CentralMessage::RejectAuthentication { .. } => {
            warn!("Received authentication message");
        }
    }
}
