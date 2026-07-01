use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};

use super::common::{authentication::Authentication, request_info::RequestInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BotMessage {
    Authorize(Authentication),
    WorkRequestMake {
        info: RequestInfo,
        metadata: HashMap<String, String>,
        idempotency_key: Option<String>,
    },
    WorkRequestGetMineInProgress,
    WorkRequestAddErrors {
        request_id: Arc<str>,
        errors: Vec<String>,
    },
    WorkRequestComplete {
        request_id: Arc<str>,
    },
}

impl From<BotMessage> for super::V1Message {
    fn from(value: BotMessage) -> Self {
        Self::Bot(value)
    }
}
