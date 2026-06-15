use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::entity::{_common::ScheduledFunctionId, authed::AuthedId};

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
    },
    WaitingForRequester {
        #[serde(with = "crate::helpers::serde::bigint")]
        since: u64,
        by: String,
        message: Option<String>,
        files_data: Option<String>,
    },
    Done {
        #[serde(rename = "at")]
        at_timestamp: i64,
    },
    Failed {
        #[serde(rename = "at")]
        at_timestamp: i64,
        reason: String,
    },
}
