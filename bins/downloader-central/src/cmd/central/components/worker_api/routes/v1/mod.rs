use axum::{
    Json, Router,
    http::StatusCode,
    routing::{any, get, post},
};
use serde_json::json;

mod auth;
mod events;
mod root;

pub fn create_v1_router() -> Router {
    Router::new()
        .route("/join-ticket", get(root::get_join_ticket))
        .route("/metrics", get(root::get_metrics))
        .route("/ws", any(root::any_ws))
        .route("/auth/token", post(auth::post_token))
        .route("/auth/refresh", post(auth::post_refresh))
        .route(
            "/watch/work-requests/{id}",
            any(root::get_work_request_watch),
        )
        .route(
            "/watch/work-requests",
            any(root::get_work_requests_watch_mine),
        )
        .route("/events/mine", any(events::any_mine))
}

#[derive(Debug)]
pub enum V1Response<T> {
    Ok(T),
    Err(StatusCode, String),
}

#[allow(dead_code)]
impl<T> V1Response<T> {
    pub const fn ok(data: T) -> Self {
        Self::Ok(data)
    }

    pub fn err<TErr>(status_code: StatusCode, error: TErr) -> Self
    where
        TErr: Into<String>,
    {
        Self::Err(status_code, error.into())
    }
}

impl<T: serde::Serialize> axum::response::IntoResponse for V1Response<T> {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Ok(data) => Json(json!({
                "status": "ok",
                "data": data,
            }))
            .into_response(),
            Self::Err(status_code, error) => (
                status_code,
                Json(json!({
                    "status": "error",
                    "error": error,
                })),
            )
                .into_response(),
        }
    }
}
