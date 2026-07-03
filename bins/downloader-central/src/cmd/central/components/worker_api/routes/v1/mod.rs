use axum::{Json, Router, routing::get};
use serde_json::json;

pub mod root;

pub fn create_v1_router() -> Router {
    Router::new()
        .route("/join-ticket", get(root::get_join_ticket))
        .route("/connections", get(root::get_connections))
        .route("/metrics", get(root::get_metrics))
}

#[derive(Debug)]
pub enum V1Response<T> {
    Ok(T),
    Err(http::StatusCode, String),
}

impl<T> V1Response<T> {
    pub const fn ok(data: T) -> Self {
        Self::Ok(data)
    }

    pub fn err<TErr>(status_code: http::StatusCode, error: TErr) -> Self
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
