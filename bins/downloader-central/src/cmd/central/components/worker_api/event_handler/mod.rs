use std::{net::SocketAddr, sync::Arc};

use app_peer_comms::message::v1::worker::CommunicationType;
use axum::extract::ws::{self, WebSocket};
use futures::{SinkExt, StreamExt};
use socket_sender::SocketSender;
use tracing::{Instrument, debug, warn};

use crate::cmd::central::components::worker_api::{
    auth::ValidAuth, request::negotiated::Negotiated,
};

mod handlers;

pub(super) mod socket_sender;

#[tracing::instrument(name = "worker-ws", skip_all, fields(id = %auth.authed_id, addr = %addr, coms = ?negotiated.comm_type()))]
pub async fn handle_socket(
    socket: WebSocket,
    auth: ValidAuth,
    addr: SocketAddr,
    negotiated: Negotiated,
) {
    let (mut sender, receiver) = socket.split();

    let until_expiry = auth.until_expiry();

    if until_expiry.is_zero() {
        _ = sender
            .send(ws::Message::Close(Some(ws::CloseFrame {
                code: ws::close_code::POLICY,
                reason: "Token expired".into(),
            })))
            .await;

        return;
    }

    let sender = Arc::from(SocketSender::new(sender, negotiated.comm_type()));

    let reqs = app_peer_comms::message::v1::central::CentralMessage::work_requests(
        crate::cmd::central::components::database::LATEST_WORKER_REQUESTS
            .read()
            .await
            .iter(),
    );
    let reqs = match reqs {
        Ok(x) => x,
        Err(e) => {
            warn!(?e, "Failed to transform work requests");
            return;
        }
    };

    _ = sender.send_message(reqs).await;

    let mut js = tokio::task::JoinSet::new();

    js.spawn(handlers::expiry::wait_for_expire(sender.clone(), until_expiry).in_current_span());
    js.spawn(
        handlers::broadcast::handle_broadcasts(sender.clone(), auth.clone()).in_current_span(),
    );
    js.spawn(
        handlers::socket_message::handle_socket_messages(sender, receiver, auth.clone())
            .in_current_span(),
    );

    js.join_next().await;
    js.abort_all();
    while js.join_next().await.is_some() {}

    debug!(?addr, "Websocket connection closed");
}

pub async fn handle_work_request_watch(
    mut socket: WebSocket,
    _auth: ValidAuth,
    request_id: String,
) {
    let reqs = app_database::Database::global()
        .requests_watch(request_id.into())
        .await;
    let mut reqs = match reqs {
        Ok(x) => x,
        Err(e) => {
            warn!(?e, "Failed to watch work request");
            return;
        }
    };

    while let Some(req) = reqs.next().await {
        let req = match req {
            Ok(x) => x,
            Err(e) => {
                warn!(?e, "Failed to watch work request");
                return;
            }
        };

        let Some(req) = req else {
            warn!("Work request deleted");
            break;
        };

        let msg: app_peer_comms::message::v1::central::work_request::WorkRequest =
            match req.try_into() {
                Ok(x) => x,
                Err(e) => {
                    warn!(?e, "Failed to transform work request");
                    break;
                }
            };

        _ = socket
            .send(ws::Message::Text(
                serde_json::to_string(&msg)
                    .expect("Failed to serialize message")
                    .into(),
            ))
            .await;
    }
}

pub async fn handle_work_requests_watch_mine_in_progress(mut socket: WebSocket, auth: ValidAuth) {
    let reqs = app_database::Database::global()
        .requests_watch_mine_in_progress(auth.authed_id.clone())
        .await;

    let mut reqs = match reqs {
        Ok(x) => x,
        Err(e) => {
            warn!(?e, "Failed to watch work request");
            return;
        }
    };

    while let Some(reqs) = reqs.next().await {
        let reqs = match reqs {
            Ok(x) => x,
            Err(e) => {
                warn!(?e, "Failed to watch work request");
                return;
            }
        };

        let msg: Arc<[app_peer_comms::message::v1::central::work_request::WorkRequest]> =
            match reqs.iter().map(std::convert::TryInto::try_into).collect() {
                Ok(x) => x,
                Err(e) => {
                    warn!(?e, "Failed to transform work requests");
                    break;
                }
            };

        _ = socket
            .send(ws::Message::Text(
                serde_json::to_string(&msg)
                    .expect("Failed to serialize message")
                    .into(),
            ))
            .await;
    }
}

impl Negotiated {
    pub const fn comm_type(&self) -> CommunicationType {
        match self {
            Self::Json => CommunicationType::Json,
            Self::Postcard => CommunicationType::Postcard,
        }
    }
}
