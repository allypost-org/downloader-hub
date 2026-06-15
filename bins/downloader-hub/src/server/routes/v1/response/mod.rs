use axum::{
    Json,
    extract::rejection::{JsonRejection, QueryRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;

use crate::server::{
    app_helpers::pagination::Paginated,
    app_response::{ApiResponse, error::ApiError},
};

pub const API_VERSION: &str = "v1";

pub type V1Result<TData = serde_json::Value> = Result<V1Response<TData>, V1Error>;

#[derive(Debug)]
pub enum V1Response<TData: Serialize + Send = serde_json::Value> {
    Success(TData),
    Error(StatusCode, ApiError),
    ErrorEmpty(StatusCode),
}
impl<TData: Serialize + Send> V1Response<TData> {
    pub fn success<T>(data: T) -> Self
    where
        T: Into<TData>,
    {
        Self::Success(data.into())
    }
}
impl V1Response<String> {
    pub const fn empty(status_code: StatusCode) -> V1Error {
        V1Error::ErrorEmpty(status_code)
    }

    pub fn error<T>(status_code: StatusCode, error: T) -> V1Error
    where
        T: Into<ApiError>,
    {
        V1Error::Error(status_code, error.into())
    }

    pub fn not_found() -> V1Error {
        V1Error::Error(StatusCode::NOT_FOUND, "Not found".into())
    }
}

impl From<V1Response<&'static str>> for V1Response<String> {
    fn from(r: V1Response<&'static str>) -> Self {
        match r {
            V1Response::Success(data) => Self::Success(data.to_string()),
            V1Response::Error(status, error) => Self::Error(status, error),
            V1Response::ErrorEmpty(status) => Self::ErrorEmpty(status),
        }
    }
}

// unsafe impl Send for V1Response {}

impl<TData: Serialize + Send> From<V1Response<TData>> for ApiResponse<TData> {
    fn from(r: V1Response<TData>) -> Self {
        let resp = Self::new(API_VERSION, StatusCode::OK);

        match r {
            V1Response::Success(data) => resp.with_success_body(data),
            V1Response::Error(status, error) => {
                resp.with_status_code(status).with_error_body(error)
            }
            V1Response::ErrorEmpty(status) => resp.with_status_code(status),
        }
    }
}

impl<TData: Serialize + Send> IntoResponse for V1Response<TData> {
    fn into_response(self) -> Response {
        let resp: ApiResponse<TData> = self.into();

        (resp.meta.status_code, Json(resp)).into_response()
    }
}

impl From<JsonRejection> for V1Response {
    fn from(rejection: JsonRejection) -> Self {
        Self::Error(rejection.status(), rejection.body_text().into())
    }
}

impl From<QueryRejection> for V1Response {
    fn from(rejection: QueryRejection) -> Self {
        Self::Error(rejection.status(), rejection.body_text().into())
    }
}

impl<TData: Serialize + Send> From<Paginated<TData>> for V1Response<Paginated<TData>> {
    fn from(paginated: Paginated<TData>) -> Self {
        Self::Success(paginated)
    }
}

#[derive(Debug, Error)]
pub enum V1Error {
    #[error(transparent)]
    Json(#[from] JsonRejection),
    #[error(transparent)]
    Query(#[from] QueryRejection),
    #[error(transparent)]
    DatabaseError(#[from] sea_orm::DbErr),
    #[error(transparent)]
    TransactionError(#[from] sea_orm::TransactionError<sea_orm::DbErr>),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{1:?}")]
    Error(StatusCode, ApiError),
    #[error("")]
    ErrorEmpty(StatusCode),
    #[error(transparent)]
    MultipartError(#[from] axum_extra::extract::multipart::MultipartError),
}
impl V1Error {
    pub fn not_found() -> Self {
        Self::Error(StatusCode::NOT_FOUND, "Not found".into())
    }
}

impl IntoResponse for V1Error {
    fn into_response(self) -> Response {
        match self {
            Self::Json(rejection) => {
                V1Response::<String>::Error(rejection.status(), rejection.body_text().into())
                    .into_response()
            }
            Self::DatabaseError(err) => {
                V1Response::<String>::Error(StatusCode::INTERNAL_SERVER_ERROR, err.into())
                    .into_response()
            }
            Self::TransactionError(err) => {
                let err = match err {
                    sea_orm::TransactionError::Transaction(err)
                    | sea_orm::TransactionError::Connection(err) => err,
                };

                V1Response::<String>::Error(StatusCode::INTERNAL_SERVER_ERROR, err.into())
                    .into_response()
            }
            Self::Io(err) => {
                V1Response::<String>::Error(StatusCode::INTERNAL_SERVER_ERROR, err.into())
                    .into_response()
            }
            Self::Query(rejection) => {
                V1Response::<String>::Error(rejection.status(), rejection.body_text().into())
                    .into_response()
            }
            Self::Error(status, error) => {
                V1Response::<String>::Error(status, error).into_response()
            }
            Self::ErrorEmpty(status) => V1Response::<String>::ErrorEmpty(status).into_response(),
            Self::MultipartError(err) => {
                V1Response::<String>::Error(err.status(), err.body_text().into()).into_response()
            }
        }
    }
}
