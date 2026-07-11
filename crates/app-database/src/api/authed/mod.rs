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
fn normalize_api_token(token: Arc<str>) -> Arc<str> {
    let trimmed = token.trim();
    if trimmed.is_empty() || trimmed.len() == token.len() {
        token
    } else {
        Arc::from(trimmed)
    }
}

impl Database {
    pub async fn authed_get_info_by_token(
        &self,
        token: Arc<str>,
    ) -> Result<AuthedInfoResponse, DatabaseError> {
        let token = normalize_api_token(token);
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthedFullInfo {
    pub id: Arc<str>,
    pub name: Arc<str>,
    #[serde(rename = "for")]
    pub for_role: AuthedForRole,
    pub readonly: bool,
    #[serde(default)]
    pub only_tagged: Arc<[String]>,
    #[serde(with = "crate::helpers::serde::bigint::option", default)]
    pub expires_at: Option<u64>,
}
impl Database {
    pub async fn authed_list_full(&self) -> Result<Arc<[AuthedFullInfo]>, DatabaseError> {
        DatabaseRequest::named("authed:listFull").query(self).await
    }

    /// Live subscription to the full authed list (including names). Emits only
    /// when the `authed:listFull` *result* changes — cheaper and more correct
    /// than watching `authed:getAll` (which carries no names) and re-fetching.
    pub async fn authed_watch_full(
        &self,
    ) -> Result<
        impl futures::stream::Stream<Item = Result<Arc<[AuthedFullInfo]>, ResponseError>>,
        DatabaseError,
    > {
        DatabaseRequest::named("authed:listFull")
            .watch_query(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthedCreateInfo {
    pub id: Arc<str>,
    pub token: Arc<str>,
}

impl Database {
    pub async fn authed_create(
        &self,
        name: &str,
        for_role: AuthedForRole,
        readonly: bool,
        only_tagged: Option<Vec<String>>,
        expires_at: Option<i64>,
    ) -> Result<AuthedCreateInfo, DatabaseError> {
        let for_str: &'static str = (&for_role).into();
        let mut req = DatabaseRequest::named("authed:create")
            .with_arg("name", name)
            .with_arg("for", for_str)
            .with_arg("readonly", readonly);
        if let Some(tags) = only_tagged {
            req = req.with_arg(
                "onlyTagged",
                tags.into_iter()
                    .map(Into::into)
                    .collect::<Vec<convex::Value>>(),
            );
        }
        if let Some(expires_at) = expires_at {
            req = req.with_arg("expiresAt", expires_at);
        }
        req.mutate(self).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum AuthedRotateTokenResult {
    NotFound,
    #[serde(rename_all = "camelCase")]
    Ok {
        token: Arc<str>,
    },
}
impl Database {
    pub async fn authed_rotate_token(
        &self,
        id: Arc<str>,
    ) -> Result<AuthedRotateTokenResult, DatabaseError> {
        DatabaseRequest::named("authed:rotateToken")
            .with_arg("id", id.as_ref())
            .mutate(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum AuthedRevokeResult {
    NotFound,
    Ok,
}
impl Database {
    pub async fn authed_revoke(&self, id: Arc<str>) -> Result<AuthedRevokeResult, DatabaseError> {
        DatabaseRequest::named("authed:revoke")
            .with_arg("id", id.as_ref())
            .mutate(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum AuthedRemoveResult {
    NotFound,
    Ok,
}
impl Database {
    pub async fn authed_remove(&self, id: Arc<str>) -> Result<AuthedRemoveResult, DatabaseError> {
        DatabaseRequest::named("authed:remove")
            .with_arg("id", id.as_ref())
            .mutate(self)
            .await
    }
}
