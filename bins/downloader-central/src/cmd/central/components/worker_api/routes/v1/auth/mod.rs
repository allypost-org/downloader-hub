use app_database::{Database, api::authed::AuthedInfoResponse};
use app_peer_comms::jwt;
use axum::{Json, response::IntoResponse};
use tracing::{debug, error};

use crate::cmd::central::components::worker_api::global::GlobalData;

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostTokenBody {
    api_key: String,
}
pub async fn post_token(Json(body): Json<PostTokenBody>) -> impl IntoResponse {
    debug!(?body, "Received token request");

    let info = match Database::global()
        .authed_get_info_by_token(body.api_key.into())
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

    let info = match info {
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

    super::V1Response::ok(jwts)
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostRefreshBody {
    refresh_token: String,
}
pub async fn post_refresh(Json(body): Json<PostRefreshBody>) -> impl IntoResponse {
    debug!(?body, "Received refresh request");

    let token_data = jwt::targeted::TargetedJwtClaims::parse(
        Some(jwt::targeted::TargetedJwtClaims::refresh_audience()),
        &body.refresh_token,
        GlobalData::jwt_secret().as_bytes(),
    );
    let token_data = match token_data {
        Ok(x) => x,
        Err(e) => {
            error!(?e, "Failed to parse refresh token");
            return super::V1Response::err(http::StatusCode::UNAUTHORIZED, "Invalid refresh token");
        }
    };

    if !token_data.is_refresh() {
        return super::V1Response::err(
            http::StatusCode::UNAUTHORIZED,
            "Invalid refresh token type",
        );
    }

    let info = match Database::global()
        .authed_get_info_by_id(token_data.id.clone())
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

    let info = match info {
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

    super::V1Response::ok(jwts)
}
