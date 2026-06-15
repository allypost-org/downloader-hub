use std::sync::Arc;

use axum::{
    extract::{WebSocketUpgrade, ws::WebSocket},
    http::HeaderMap,
    response::IntoResponse,
};
use futures::StreamExt;
use tracing::{debug, trace, warn};

use crate::cmd::central::{
    auth::ValidAuth,
    broadcaster::{BroadcastAudience, Broadcaster},
    components::worker_api::{
        event_handler::socket_sender::SocketSender, request::negotiated::Negotiated,
    },
};

pub async fn any_mine(
    auth: ValidAuth,
    ws: WebSocketUpgrade,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Some(negotiated) = Negotiated::negotiate(&headers) else {
        return super::V1Response::<()>::err(
            reqwest::StatusCode::NOT_ACCEPTABLE,
            "No acceptable content type",
        )
        .into_response();
    };

    ws.on_upgrade(move |socket| {
        let auth = auth;
        async move {
            debug!(?auth, "My Events WS connection opened");
            handle_my_events(auth, socket, negotiated).await;
        }
    })
}

async fn handle_my_events(auth: ValidAuth, socket: WebSocket, negotiated: Negotiated) {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();

    let mut js = tokio::task::JoinSet::new();
    let authed_id = auth.authed_id.clone();

    let socket_sender = SocketSender::new(socket.split().0, negotiated.comm_type());

    js.spawn({
        let sender = sender.clone();
        let authed_id = authed_id.clone();

        async move {
            let db_reqs = app_database::Database::global()
                .requests_watch_mine_in_progress(authed_id.clone())
                .await;

            let mut db_reqs = match db_reqs {
                Ok(x) => x,
                Err(e) => {
                    warn!(?e, "Failed to watch work request");
                    return;
                }
            };

            while let Some(reqs) = db_reqs.next().await {
                let reqs = match reqs {
                    Ok(x) => x,
                    Err(e) => {
                        warn!(?e, "Failed to watch work request");
                        return;
                    }
                };

                let msgs = reqs.iter().map(|x| {
                    app_peer_comms::message::v1::central::work_request::WorkRequest::try_from(x)
                        .map(app_peer_comms::message::v1::central::CentralMessage::from)
                });

                for msg in msgs {
                    let msg = match msg {
                        Ok(x) => x,
                        Err(e) => {
                            warn!(?e, "Failed to transform work request");
                            continue;
                        }
                    };

                    let Ok(()) = sender.send(Arc::new(msg)) else {
                        trace!("RX channel got closed");
                        break;
                    };
                }
            }
        }
    });

    js.spawn(async move {
        let mut broadcast_receiver = Broadcaster::recv_from_now();

        while let Ok(msg) = broadcast_receiver.recv().await {
            let Some(msg) = msg.get_if_for_me(&[BroadcastAudience::Authed(authed_id.clone())])
            else {
                continue;
            };

            let Ok(()) = sender.send(msg) else {
                trace!("RX channel got closed");
                return;
            };
        }

        trace!("Broadcast receiver got closed");
    });

    tokio::select! {
        Some(msg) = receiver.recv() => {
            debug!(?msg, "Received message");

            if let Err(e) = socket_sender.send_message_boxed(msg.clone()).await {
                warn!(?e, ?msg, "Failed to send message");
            }
        }
        _ = js.join_next() => {
            debug!("Some task finished early. Exiting.");
            js.abort_all();
        }
    }
}
