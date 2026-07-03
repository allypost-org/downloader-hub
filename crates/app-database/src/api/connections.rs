use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{Database, DatabaseError, DatabaseRequest};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub central: String,
    pub authed: Arc<str>,
    pub role: String,
    #[serde(default)]
    pub capabilities: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(with = "crate::helpers::serde::bigint")]
    pub last_seen: u64,
}

impl Database {
    pub async fn connections_upsert(
        &self,
        central: String,
        authed: Arc<str>,
        role: &str,
        capabilities_json: Option<String>,
        version: Option<String>,
    ) -> Result<(), DatabaseError> {
        DatabaseRequest::named("connections:upsert")
            .with_arg("central", central)
            .with_arg("authed", authed.as_ref())
            .with_arg("role", role)
            .with_arg("capabilities", capabilities_json)
            .with_arg("version", version)
            .mutate(self)
            .await
    }

    pub async fn connections_heartbeat(
        &self,
        central: String,
        authed: Arc<str>,
    ) -> Result<(), DatabaseError> {
        DatabaseRequest::named("connections:heartbeat")
            .with_arg("central", central)
            .with_arg("authed", authed.as_ref())
            .mutate(self)
            .await
    }

    pub async fn connections_remove(
        &self,
        central: String,
        authed: Arc<str>,
    ) -> Result<(), DatabaseError> {
        DatabaseRequest::named("connections:remove")
            .with_arg("central", central)
            .with_arg("authed", authed.as_ref())
            .mutate(self)
            .await
    }

    pub async fn connections_list(&self) -> Result<Vec<ConnectionInfo>, DatabaseError> {
        DatabaseRequest::named("connections:list").query(self).await
    }
}
