use std::{collections::HashMap, sync::Arc};

use app_database::entity::accounts::{AccountPlace, AccountPlaceRef, AccountUser, AccountUserRef};
use serde::{Deserialize, Serialize};

use crate::{
    message::v1::common::{RequestId, file::FileReference, request_info::RequestInfo},
    rpc::session::Role,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandlerEntry {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitiesSummary {
    #[serde(default)]
    pub extractors: Vec<HandlerEntry>,
    #[serde(default)]
    pub downloaders: Vec<HandlerEntry>,
    #[serde(default)]
    pub fixers: Vec<HandlerEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capabilities {
    Worker {
        #[serde(default)]
        extractors: Vec<HandlerEntry>,
        #[serde(default)]
        downloaders: Vec<HandlerEntry>,
        #[serde(default)]
        fixers: Vec<HandlerEntry>,
    },
    Bot {
        platform: String,
    },
    Admin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Auth {
    pub api_key: Arc<str>,
    pub capabilities: Capabilities,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Heartbeat;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetWorkItem;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefuseWorkItem {
    pub request_id: RequestId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkRequestFree {
    pub request_id: RequestId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkRequestUpdateStatus {
    pub request_id: RequestId,
    pub message: Arc<str>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkRequestAddErrors {
    pub request_id: RequestId,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkRequestMoveToWaiting {
    pub request_id: RequestId,
    pub files_data: Vec<FileReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkRequestFail {
    pub request_id: RequestId,
    pub reason: Arc<str>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkRequestMake {
    pub info: RequestInfo,
    pub metadata: HashMap<String, String>,
    pub idempotency_key: Option<String>,
    #[serde(default)]
    pub ordered_by: Option<AccountUserRef>,
    #[serde(default)]
    pub ordered_in: Option<AccountPlaceRef>,
}

/// Refresh metadata for end-users / places.
///
/// Bots call this on each client message (where the freshest data is
/// available). Central upserts into the `account_users` / `account_places`
/// tables. Idempotent on `(platform, platformId)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsUpsert {
    #[serde(default)]
    pub users: Vec<AccountUser>,
    #[serde(default)]
    pub places: Vec<AccountPlace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsUpsertResult {
    pub users: u64,
    pub places: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkRequestComplete {
    pub request_id: RequestId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkRequestGetMineInProgress;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCapabilities;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminListSessions;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminSessionInfo {
    pub authed_id: Arc<str>,
    pub role: Role,
    pub connected_at: u64,
    pub expires_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AdminSessionsResult {
    Unauthorized,
    Ok(Vec<AdminSessionInfo>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminListParkedWorkers;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminParkedWorker {
    pub authed_id: Arc<str>,
    pub since: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AdminParkedWorkersResult {
    Unauthorized,
    Ok(Vec<AdminParkedWorker>),
}
