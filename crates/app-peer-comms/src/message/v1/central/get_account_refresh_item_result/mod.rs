use serde::{Deserialize, Serialize};

use crate::message::v1::central::work_request::request::WorkRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GetAccountRefreshItemResult {
    Ok(Box<WorkRequest>),
    NoWork,
    BackendError,
    Unauthorized,
}

impl GetAccountRefreshItemResult {
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }
}
