use std::net::SocketAddr;

use app_database::{Database, api::authed::AuthedInfoResponse, entity::authed::AuthedForRole};
use app_peer_comms::{
    PeeringEndpoint, jwt,
    ticket::targeted::{TargetedTicket, TicketTarget},
};
use axum::{
    extract::{ConnectInfo, Path, WebSocketUpgrade, ws::WebSocket},
    http::HeaderMap,
    response::IntoResponse,
};
use tracing::{debug, error, trace};

use crate::cmd::central::components::worker_api::{
    auth::ValidAuth, event_handler, global::GlobalData, request::negotiated::Negotiated,
};

pub async fn get_join_ticket(headers: HeaderMap) -> impl IntoResponse {
    let pe = PeeringEndpoint::global();

    let Some(auth_header) = headers.get("Authorization").and_then(|v| v.to_str().ok()) else {
        return super::V1Response::err(
            http::StatusCode::UNAUTHORIZED,
            "Missing Authorization header",
        );
    };
    let Some((auth_type, token)) = auth_header.split_once(' ') else {
        return super::V1Response::err(
            http::StatusCode::UNAUTHORIZED,
            "Invalid Authorization header: must be `Bearer <token>`",
        );
    };

    if auth_type.to_lowercase() != "bearer" {
        return super::V1Response::err(
            http::StatusCode::UNAUTHORIZED,
            "Invalid Authorization header: must be `Bearer <token>`",
        );
    }

    let token = token.trim();

    let res = match Database::global()
        .authed_get_info_by_token(token.into())
        .await
    {
        Ok(info) => info,
        Err(e) => {
            error!(?e, "Failed to get authed info");
            return super::V1Response::err(
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong while checking token",
            );
        }
    };

    let info = match res {
        AuthedInfoResponse::NotAuthorized { error } => {
            return super::V1Response::err(http::StatusCode::UNAUTHORIZED, error);
        }
        AuthedInfoResponse::Authorized(info) => info,
    };

    let jwts = jwt::targeted::TargetedJwtPair::generate(
        &jwt::targeted::TargetedJwtConfig::new(info.id.clone(), info.for_role.to_string().into())
            .with_expires_at(
                info.expires_at
                    .and_then(chrono::DateTime::<chrono::Utc>::from_timestamp_micros),
            ),
        GlobalData::jwt_secret().as_bytes(),
    );

    let jwts = match jwts {
        Ok(x) => x,
        Err(e) => {
            error!(?e, "Failed to generate JWTs");
            return super::V1Response::err(
                http::StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong while generating JWTs",
            );
        }
    };

    let ticket = TargetedTicket::new(pe.join_ticket().await);

    super::V1Response::ok(serde_json::json!({
        "jwt_token": jwts.token(),
        "refresh_token": jwts.refresh_token(),
        "ticket": ticket.to_string(match info.for_role {
            AuthedForRole::Worker => TicketTarget::Worker,
            AuthedForRole::Bot => TicketTarget::Bot,
        }),
    }))
}

pub async fn get_metrics() -> impl IntoResponse {
    let pe = PeeringEndpoint::global();

    let endpoint = pe.router.endpoint().metrics();
    // let gossip = pe.gossip.metrics();

    super::V1Response::ok(serde_json::json!({
        "endpoint": endpoint,
        // "gossip": gossip,
    }))
}

pub async fn any_ws(
    auth: ValidAuth,
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    let Some(negotiated) = Negotiated::negotiate(&headers) else {
        return super::V1Response::<()>::err(
            http::StatusCode::NOT_ACCEPTABLE,
            "No acceptable content type",
        )
        .into_response();
    };

    ws.on_upgrade(move |socket| handle_socket(socket, auth, addr, negotiated))
        .into_response()
}

async fn handle_socket(
    mut socket: WebSocket,
    auth: ValidAuth,
    who: SocketAddr,
    negotiated: Negotiated,
) {
    debug!(?auth, ?who, "Websocket connection opened");

    if socket
        .send(axum::extract::ws::Message::Ping(
            axum::body::Bytes::from_static(&[1, 2, 3]),
        ))
        .await
        .is_ok()
    {
        trace!(?who, "Pinged");
    } else {
        trace!(?who, "Could not send ping");
        // no Error here since the only thing we can do is to close the connection.
        // If we can not send messages, there is no way to salvage the statemachine anyway.
        return;
    }

    event_handler::handle_socket(socket, auth, who, negotiated).await;
}

pub async fn get_work_request_watch(
    auth: ValidAuth,
    ws: WebSocketUpgrade,
    Path(request_id): Path<String>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| {
        let auth = auth;
        async move {
            debug!(?auth, "Websocket connection opened");
            event_handler::handle_work_request_watch(socket, auth, request_id).await;
        }
    })
}

pub async fn get_work_requests_watch_mine(
    auth: ValidAuth,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| {
        let auth = auth;
        async move {
            debug!(?auth, "Websocket connection opened");
            event_handler::handle_work_requests_watch_mine_in_progress(socket, auth).await;
        }
    })
}
