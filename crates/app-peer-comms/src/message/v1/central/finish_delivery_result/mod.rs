use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkRequestFinishDeliveryResult {
    Ok,
    NotDelivering,
    StaleAttempt,
    NotFound,
    Unauthorized,
    BackendError,
}
