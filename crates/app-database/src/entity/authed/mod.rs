use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub type AuthedId = Arc<str>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Authed {
    pub name: String,
    pub token: String,
    pub readonly: bool,
    #[serde(rename = "for")]
    pub for_role: AuthedForRole,
    pub only_tagged: Option<Vec<String>>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthedForRole {
    #[serde(rename = "worker")]
    Worker,
    #[serde(rename = "bot")]
    Bot,
    #[serde(rename = "admin")]
    Admin,
}

impl From<AuthedForRole> for &'static str {
    fn from(value: AuthedForRole) -> Self {
        (&value).into()
    }
}

impl From<&AuthedForRole> for &'static str {
    fn from(value: &AuthedForRole) -> Self {
        match value {
            AuthedForRole::Worker => "worker",
            AuthedForRole::Bot => "bot",
            AuthedForRole::Admin => "admin",
        }
    }
}

impl From<AuthedForRole> for Arc<str> {
    fn from(value: AuthedForRole) -> Self {
        (&value).into()
    }
}

impl From<&AuthedForRole> for Arc<str> {
    fn from(value: &AuthedForRole) -> Self {
        let value: &'static str = value.into();

        Self::from(value)
    }
}

impl std::fmt::Display for AuthedForRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s: &'static str = self.into();
        s.fmt(f)
    }
}
