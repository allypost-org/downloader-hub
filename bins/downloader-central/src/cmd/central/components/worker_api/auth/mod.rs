use app_peer_comms::jwt;
use axum::{
    Json, RequestPartsExt,
    extract::FromRequestParts,
    http::request::Parts,
    response::{IntoResponse, Response},
};
use axum_extra::{
    TypedHeader,
    headers::{Authorization, authorization::Bearer},
};
use http::StatusCode;
use serde_json::json;

pub use crate::cmd::central::auth::{AuthError, ValidAuth};
use crate::cmd::central::components::worker_api::global::GlobalData;

impl<S> FromRequestParts<S> for ValidAuth
where
    S: Send + Sync,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract the token from the authorization header
        let TypedHeader(Authorization(bearer)) = parts
            .extract::<TypedHeader<Authorization<Bearer>>>()
            .await
            .map_err(|_| AuthError::MissingHeader)?;

        // Decode the user data
        let token_data = jwt::targeted::TargetedJwtClaims::parse(
            None,
            bearer.token(),
            GlobalData::jwt_secret().as_bytes(),
        )
        .map_err(|e| AuthError::InvalidToken(e.to_string()))?;

        if token_data.is_refresh() {
            return Err(AuthError::InvalidToken(
                "Invalid token, not a worker token".to_string(),
            ));
        }

        Ok(Self {
            authed_id: token_data.id.clone(),
            expires_at: token_data.expires_at,
        })
    }
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            Self::MissingHeader => (
                StatusCode::UNAUTHORIZED,
                "Missing or invalid auth header".into(),
            ),
            Self::InvalidToken(e) => (StatusCode::BAD_REQUEST, format!("Invalid token: {e}")),
        };
        let body = Json(json!({
            "error": error_message,
        }));
        (status, body).into_response()
    }
}
