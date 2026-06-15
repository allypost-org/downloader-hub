use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use self::error::ApiError;

pub mod error;
pub mod range_responder;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::pub_underscore_fields)]
pub struct ApiResponse<TBody = ()> {
    /// Response metadata
    ///
    /// This is the metadata of the response.
    ///
    /// It contains information such as the API version, timestamp, and status code.
    pub meta: ApiResponseMeta,

    #[cfg(debug_assertions)]
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Debug information
    ///
    /// _This is only present in debug builds._
    pub _debug: Option<serde_json::Value>,

    /// Response body
    ///
    /// This is the response body.
    pub body: ApiResponseBody<TBody>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StatusCodeSerializer;
impl StatusCodeSerializer {
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn serialize<S>(status: &StatusCode, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u16(status.as_u16())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<StatusCode, D::Error>
    where
        D: Deserializer<'de>,
    {
        let code = u16::deserialize(deserializer)?;
        StatusCode::from_u16(code).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponseMeta {
    /// Timestamp in RFC3339 format
    ///
    /// Signifies the time at which the response was generated.
    /// This is not the time of the request.
    #[serde(rename = "at")]
    pub timestamp: String,

    /// API version
    ///
    /// This is the version of the API that was used to generate the response.
    /// This is not the version of the response itself.
    /// v0 is used for non-versioned/base responses.
    #[serde(rename = "v")]
    pub api_version: String,

    /// HTTP status code
    ///
    /// This is the HTTP status code of the response.
    /// Placed here for convenience.
    #[serde(rename = "status", with = "StatusCodeSerializer")]
    pub status_code: StatusCode,
}

fn serialize_empty<S>(serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_none()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "camelCase")]
pub enum ApiResponseBody<TData = ()> {
    #[serde(serialize_with = "serialize_empty")]
    Empty,
    Success(TData),
    Error(ApiError),
}

impl<TData: Serialize> ApiResponse<TData> {
    pub fn new<TVersion: Into<String>>(api_version: TVersion, status_code: StatusCode) -> Self {
        Self {
            meta: ApiResponseMeta {
                api_version: api_version.into(),
                status_code,
                timestamp: chrono::Utc::now().to_rfc3339(),
            },
            #[cfg(debug_assertions)]
            _debug: None,
            body: ApiResponseBody::Empty,
        }
    }

    pub fn not_found(api_version: String) -> Self {
        Self::new(api_version, StatusCode::NOT_FOUND)
    }

    pub const fn with_status_code(mut self, status_code: StatusCode) -> Self {
        self.meta.status_code = status_code;
        self
    }

    pub fn with_body(mut self, body: ApiResponseBody<TData>) -> Self {
        self.body = body;
        self
    }

    pub fn with_success_body(self, body: TData) -> Self {
        self.with_body(ApiResponseBody::Success(body))
    }

    pub fn with_error_body(self, error: ApiError) -> Self {
        self.with_body(ApiResponseBody::Error(error))
    }

    #[cfg_attr(not(debug_assertions), allow(unused_mut, unused_variables))]
    pub fn with_debug(mut self, debug: serde_json::Value) -> Self {
        #[cfg(debug_assertions)]
        #[allow(clippy::used_underscore_binding)]
        {
            self._debug = Some(debug);
        }

        self
    }
}

impl ApiResponse<()> {
    pub fn empty<TVersion: Into<String>>(api_version: TVersion, status_code: StatusCode) -> Self {
        Self::new(api_version, status_code)
    }
}

impl IntoResponse for &ApiResponse<serde_json::Value> {
    fn into_response(self) -> Response {
        (self.meta.status_code, Json(self)).into_response()
    }
}

impl<TData> IntoResponse for ApiResponse<TData>
where
    TData: Serialize + Send,
{
    fn into_response(self) -> Response {
        (self.meta.status_code, Json(self)).into_response()
    }
}

impl<TData> From<TData> for ApiResponse<TData>
where
    TData: Serialize + Send,
{
    fn from(s: TData) -> Self {
        Self::new("v0", StatusCode::OK).with_success_body(s)
    }
}
