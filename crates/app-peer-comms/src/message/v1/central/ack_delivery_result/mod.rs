use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::message::v1::common::file::FileReference;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkRequestAckResult {
    Claimed {
        delivery_attempt_id: Arc<str>,
        files: Arc<[FileReference]>,
    },
    NotWaitingForRequester,
    AlreadyDelivering,
    NotFound,
    Unauthorized,
    BackendError,
}
