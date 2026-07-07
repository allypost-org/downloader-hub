use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::Serialize;

#[derive(Debug)]
pub enum V1Response<T> {
    Ok(T),
    Err(StatusCode, String),
}

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

impl<T: Serialize> IntoResponse for V1Response<T> {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Ok(data) => Json(serde_json::json!({
                "status": "ok",
                "data": data,
            }))
            .into_response(),
            Self::Err(status_code, error) => (
                status_code,
                Json(serde_json::json!({
                    "status": "error",
                    "error": error,
                })),
            )
                .into_response(),
        }
    }
}
