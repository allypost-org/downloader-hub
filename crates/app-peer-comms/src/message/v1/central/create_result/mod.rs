use std::sync::Arc;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CreateResult {
    Ok(CreateResultData),
    BackendError,
    Unauthorized,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateResultData {
    pub id: Arc<str>,
}
