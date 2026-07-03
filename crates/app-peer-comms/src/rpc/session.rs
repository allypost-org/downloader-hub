use std::sync::Arc;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Role {
    #[serde(rename = "worker")]
    Worker,
    #[serde(rename = "bot")]
    Bot,
}

impl From<app_database::entity::authed::AuthedForRole> for Role {
    fn from(value: app_database::entity::authed::AuthedForRole) -> Self {
        match value {
            app_database::entity::authed::AuthedForRole::Worker => Self::Worker,
            app_database::entity::authed::AuthedForRole::Bot => Self::Bot,
        }
    }
}

impl From<&app_database::entity::authed::AuthedForRole> for Role {
    fn from(value: &app_database::entity::authed::AuthedForRole) -> Self {
        match value {
            app_database::entity::authed::AuthedForRole::Worker => Self::Worker,
            app_database::entity::authed::AuthedForRole::Bot => Self::Bot,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthedInfo {
    pub id: Arc<str>,
    pub role: Role,
    #[serde(default)]
    pub expires_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub info: AuthedInfo,
}

impl Session {
    #[must_use]
    pub const fn new(info: AuthedInfo) -> Self {
        Self { info }
    }

    #[must_use]
    pub const fn role(&self) -> &Role {
        &self.info.role
    }
}
