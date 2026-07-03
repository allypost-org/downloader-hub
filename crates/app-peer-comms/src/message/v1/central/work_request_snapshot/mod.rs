use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::message::v1::central::work_request::WorkRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkRequestSnapshot {
    pub requests: Arc<[WorkRequest]>,
}
