use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::entity::{
    _common::ScheduledFunctionId,
    accounts::{AccountPlaceRef, AccountUserRef},
    authed::AuthedId,
};

pub mod file_reference;
pub mod request_info;

pub type RequestId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    pub requester: AuthedId,
    pub info: request_info::RequestInfo,
    pub tries: i64,
    pub status: RequestStatus,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    pub last_modified: i64,
    #[serde(default)]
    pub ordered_by: Option<AccountUserRef>,
    #[serde(default)]
    pub ordered_in: Option<AccountPlaceRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "Type")]
pub enum RequestStatus {
    Pending,
    InProgress {
        #[serde(rename = "since")]
        since_timestamp: i64,
        by: AuthedId,
        #[serde(rename = "CleanupId")]
        cleanup_fn_id: ScheduledFunctionId,
        message: Option<String>,
        files_data: Option<String>,
        waiting_for_requester: Option<bool>,
    },
    Delivering {
        #[serde(with = "crate::helpers::serde::bigint")]
        since: u64,
        #[serde(with = "crate::helpers::serde::bigint")]
        worker_since: u64,
        worker_by: AuthedId,
        claimed_by: AuthedId,
        delivery_attempt_id: String,
        message: Option<String>,
        files_data: Option<String>,
        #[serde(rename = "CleanupId")]
        cleanup_fn_id: ScheduledFunctionId,
    },
    Done {
        #[serde(rename = "at")]
        at_timestamp: i64,
        by: AuthedId,
        #[serde(default)]
        delivered_by: Option<AuthedId>,
    },
    Failed {
        #[serde(rename = "at")]
        at_timestamp: i64,
        by: AuthedId,
        reason: String,
    },
}
