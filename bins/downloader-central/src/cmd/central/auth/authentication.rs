use std::sync::Arc;

use app_database::{api::authed::AuthedInfoResponse, entity::authed::AuthedForRole};
use app_peer_comms::{
    jwt::targeted::{TargetedJwtClaims, TargetedJwtConfig},
    message::v1::common::authentication::Authentication,
};
use tracing::{debug, error};

use crate::cmd::central::config::CentralConfig;

pub trait Authenticatable {
    async fn as_targeted_jwt_config(
        &self,
        for_role: AuthedForRole,
    ) -> Result<TargetedJwtConfig, String>;
}

impl Authenticatable for Authentication {
    async fn as_targeted_jwt_config(
        &self,
        for_role: AuthedForRole,
    ) -> Result<TargetedJwtConfig, String> {
        match self {
            Self::ApiKey(key) => lookup_targeted_jwt_config(DbLookup::ApiKey(key.clone())).await,

            Self::JwtPair(jwt_pair) => {
                let token_claims = TargetedJwtClaims::parse(
                    Some(for_role.clone().into()),
                    &jwt_pair.token,
                    CentralConfig::jwt_secret().as_bytes(),
                );
                if let Ok(claims) = token_claims
                    && !claims.is_refresh()
                {
                    return Ok(claims.with_audience_target(for_role.into()).into_config());
                }

                let claims = TargetedJwtClaims::parse(
                    Some(for_role.into()),
                    &jwt_pair.refresh_token,
                    CentralConfig::jwt_secret().as_bytes(),
                );

                let claims = match claims {
                    Ok(claims) => claims,
                    Err(e) => {
                        debug!(?e, "Failed to parse JWT pair");
                        return Err(format!("Failed to parse JWT pair: {}", e));
                    }
                };

                if !claims.is_refresh() {
                    return Err("Refresh token isn't refresh token".to_string());
                }

                lookup_targeted_jwt_config(DbLookup::Id(claims.id.clone())).await
            }

            Self::RefreshToken(refresh_token) => {
                let claims = TargetedJwtClaims::parse(
                    Some(for_role.into()),
                    refresh_token,
                    CentralConfig::jwt_secret().as_bytes(),
                );

                let claims = match claims {
                    Ok(claims) => claims,
                    Err(e) => {
                        error!(?e, "Failed to parse refresh token");
                        return Err(format!("Failed to parse refresh token: {}", e));
                    }
                };

                if !claims.is_refresh() {
                    error!("Refresh token is not a refresh token");
                    return Err("Refresh token is not a refresh token".to_string());
                }

                lookup_targeted_jwt_config(DbLookup::Id(claims.id.clone())).await
            }
        }
    }
}

enum DbLookup {
    Id(Arc<str>),
    ApiKey(Arc<str>),
}

async fn lookup_targeted_jwt_config(lookup: DbLookup) -> Result<TargetedJwtConfig, String> {
    debug!("Looking up targeted JWT config");

    let resp = match lookup {
        DbLookup::Id(id) => {
            app_database::Database::global()
                .authed_get_info_by_id(id)
                .await
        }
        DbLookup::ApiKey(api_key) => {
            app_database::Database::global()
                .authed_get_info_by_token(api_key)
                .await
        }
    };

    let resp = match resp {
        Ok(resp) => resp,
        Err(e) => {
            error!(?e, "Failed to authenticate");
            return Err(format!("Failed to authenticate: {}", e));
        }
    };

    let resp = match resp {
        AuthedInfoResponse::Authorized(resp) => resp,
        AuthedInfoResponse::NotAuthorized { error } => return Err(error),
    };

    debug!("Found authentication in database");

    Ok(
        TargetedJwtConfig::new(resp.id.clone(), resp.for_role.into()).with_expires_at(
            resp.expires_at
                .and_then(chrono::DateTime::from_timestamp_secs),
        ),
    )
}
