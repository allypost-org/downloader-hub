use axum::{
    extract::{Query, Request},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

pub use crate::server::app_middleware::auth::CurrentUser;
use crate::server::{
    app_middleware::auth::{AuthQueryKey, add_user_to_request, is_admin},
    routes::v1::response::V1Response,
};

pub async fn require_auth(
    Query(auth_query): Query<AuthQueryKey>,
    mut req: Request,
    next: Next,
) -> Response {
    match add_user_to_request(Some(auth_query), &mut req).await {
        Some(_) => next.run(req).await,
        None => V1Response::error(
            StatusCode::UNAUTHORIZED,
            "Missing or invalid authentication header",
        )
        .into_response(),
    }
}

pub async fn require_admin(
    Query(auth_query): Query<AuthQueryKey>,
    mut req: Request,
    next: Next,
) -> Response {
    match add_user_to_request(Some(auth_query), &mut req).await {
        Some(user) if is_admin(&user) => next.run(req).await,
        Some(_) => {
            V1Response::error(StatusCode::FORBIDDEN, "Insufficient privileges").into_response()
        }
        None => V1Response::error(
            StatusCode::UNAUTHORIZED,
            "Missing or invalid authentication header",
        )
        .into_response(),
    }
}

pub async fn require_auth_not_admin(
    Query(auth_query): Query<AuthQueryKey>,
    mut req: Request,
    next: Next,
) -> Response {
    match add_user_to_request(Some(auth_query), &mut req).await {
        Some(user) if is_admin(&user) => V1Response::error(
            StatusCode::UNAUTHORIZED,
            "Admins are not allowed to perform this action",
        )
        .into_response(),
        Some(_) => next.run(req).await,
        None => V1Response::error(
            StatusCode::UNAUTHORIZED,
            "Missing or invalid authentication header",
        )
        .into_response(),
    }
}
