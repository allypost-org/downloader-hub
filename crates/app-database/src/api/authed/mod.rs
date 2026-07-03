use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    Database, DatabaseError, DatabaseRequest, entity::authed::AuthedForRole, error::ResponseError,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "Type", rename_all = "kebab-case")]
pub enum AuthedInfoResponse {
    Authorized(AuthedInfo),
    NotAuthorized {
        #[serde(rename = "error")]
        error: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthedInfo {
    pub id: Arc<str>,
    pub name: Arc<str>,
    pub readonly: bool,
    #[serde(rename = "for")]
    pub for_role: AuthedForRole,
    #[serde(rename = "onlyTagged", default)]
    pub only_tagged: Arc<[String]>,
    #[serde(
        with = "crate::helpers::serde::bigint::option",
        default,
        rename = "expiresAt"
    )]
    pub expires_at: Option<u64>,
}
impl Database {
    pub async fn authed_get_info_by_token(
        &self,
        token: Arc<str>,
    ) -> Result<AuthedInfoResponse, DatabaseError> {
        DatabaseRequest::named("authed:getInfoByToken")
            .with_arg("token", token.as_ref())
            .query(self)
            .await
    }
}

impl Database {
    pub async fn authed_get_info_by_id(
        &self,
        id: Arc<str>,
    ) -> Result<AuthedInfoResponse, DatabaseError> {
        DatabaseRequest::named("authed:getInfoById")
            .with_arg("id", id.as_ref())
            .query(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthedIdWithExpiry {
    pub id: Arc<str>,
    #[serde(with = "crate::helpers::serde::bigint::option", default)]
    pub expires_at: Option<u64>,
}

impl Database {
    pub async fn authed_watch_all(
        &self,
    ) -> Result<
        impl futures::stream::Stream<Item = Result<Arc<[AuthedIdWithExpiry]>, ResponseError>>,
        DatabaseError,
    > {
        DatabaseRequest::named("authed:getAll")
            .watch_query(self)
            .await
    }
}
