use app_database::{Database, api::authed::AuthedInfoResponse, entity::authed::AuthedForRole};
use app_peer_comms::{
    PeeringEndpoint,
    ticket::targeted::{TargetedTicket, TicketTarget},
};
use axum::{
    http::{HeaderMap, StatusCode, header::CONTENT_TYPE},
    response::IntoResponse,
};
use tracing::error;

use crate::cmd::central::components::metrics;

pub async fn get_join_ticket(headers: HeaderMap) -> impl IntoResponse {
    let Some(info) = require_authed(&headers).await else {
        return super::V1Response::err(
            StatusCode::UNAUTHORIZED,
            "Missing or invalid `Authorization: Bearer <api_key>` header",
        );
    };

    let info = match info {
        AuthedInfoResponse::NotAuthorized { error } => {
            return super::V1Response::err(StatusCode::UNAUTHORIZED, error);
        }
        AuthedInfoResponse::Authorized(info) => info,
    };

    let pe = PeeringEndpoint::global();
    let ticket = TargetedTicket::new(pe.join_ticket().await);
    let target = match info.for_role {
        AuthedForRole::Worker => TicketTarget::Worker,
        AuthedForRole::Bot => TicketTarget::Bot,
    };

    super::V1Response::ok(serde_json::json!({
        "ticket": ticket.to_string(target),
    }))
}

pub async fn get_connections(headers: HeaderMap) -> impl IntoResponse {
    if require_authed(&headers).await.is_none() {
        return super::V1Response::err(
            StatusCode::UNAUTHORIZED,
            "Missing or invalid `Authorization: Bearer <api_key>` header",
        );
    }

    match Database::global().connections_list().await {
        Ok(rows) => super::V1Response::ok(serde_json::json!({
            "connections": rows,
        })),
        Err(e) => {
            error!(?e, "Failed to list connections");
            super::V1Response::err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong while listing connections",
            )
        }
    }
}

pub async fn get_metrics() -> impl IntoResponse {
    let body = metrics::render();
    (
        StatusCode::OK,
        [(CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
}

pub async fn get_health() -> impl IntoResponse {
    StatusCode::OK
}

async fn require_authed(headers: &HeaderMap) -> Option<AuthedInfoResponse> {
    let token = extract_bearer(headers)?;
    match Database::global()
        .authed_get_info_by_token(token.into())
        .await
    {
        Ok(info) => Some(info),
        Err(e) => {
            error!(?e, "Failed to get authed info");
            None
        }
    }
}

fn extract_bearer(headers: &HeaderMap) -> Option<&str> {
    let header = headers.get("Authorization")?.to_str().ok()?;
    let (auth_type, token) = header.split_once(' ')?;
    if !auth_type.eq_ignore_ascii_case("bearer") {
        return None;
    }
    Some(token.trim())
}
