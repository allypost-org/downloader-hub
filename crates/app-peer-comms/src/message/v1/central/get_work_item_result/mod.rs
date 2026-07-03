use serde::{Deserialize, Serialize};

use crate::message::v1::central::work_request::request::WorkRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GetWorkItemResult {
    Ok(Box<WorkRequest>),
    BackendError,
    Unauthorized,
}

impl GetWorkItemResult {
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }
}
