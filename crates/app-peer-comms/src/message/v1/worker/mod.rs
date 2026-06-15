use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::common::authentication::Authentication;
use crate::message::v1::common::file::FileReference;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkerMessage {
    Authorize(Authentication),
    WorkRequestTake {
        request_id: Arc<str>,
    },
    WorkRequestFree {
        request_id: Arc<str>,
    },
    WorkRequestUpdateStatusMessage {
        request_id: Arc<str>,
        message: Arc<str>,
    },
    WorkRequestAddErrors {
        request_id: Arc<str>,
        errors: Vec<String>,
    },
    WorkRequestMoveToWaitingForRequester {
        request_id: Arc<str>,
        files_data: Vec<FileReference>,
    },
    WorkRequestFail {
        request_id: Arc<str>,
        reason: Arc<str>,
    },
}

impl From<WorkerMessage> for super::V1Message {
    fn from(msg: WorkerMessage) -> Self {
        Self::Worker(msg)
    }
}

impl From<WorkerMessage> for super::Message {
    fn from(msg: WorkerMessage) -> Self {
        Self::V1(msg.into())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CommunicationType {
    Json,
    Postcard,
}

impl CommunicationType {
    #[must_use]
    pub const fn json() -> Self {
        Self::Json
    }

    #[must_use]
    pub const fn postcard() -> Self {
        Self::Postcard
    }
}

impl CommunicationType {
    pub fn encode<T>(&self, data: T) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>>
    where
        T: serde::Serialize,
    {
        match self {
            Self::Json => serde_json::to_vec(&data).map_err(Into::into),
            Self::Postcard => postcard::to_stdvec(&data).map_err(Into::into),
        }
    }

    pub fn decode<T>(
        &self,
        data: impl AsRef<[u8]>,
    ) -> Result<T, Box<dyn std::error::Error + Send + Sync>>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        match self {
            Self::Json => serde_json::from_slice(data.as_ref()).map_err(Into::into),
            Self::Postcard => postcard::from_bytes(data.as_ref()).map_err(Into::into),
        }
    }
}
