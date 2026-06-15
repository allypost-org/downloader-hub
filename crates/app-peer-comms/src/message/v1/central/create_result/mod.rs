use std::sync::Arc;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CreateResult {
    Ok(CreateResultData),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateResultData {
    pub id: Arc<str>,
}
