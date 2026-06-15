use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::message::v1::common::RequestId;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailResult {
    pub request_id: RequestId,
    pub result: FailResultStatus,
}

impl FailResult {
    #[must_use]
    const fn new(request_id: RequestId, result: FailResultStatus) -> Self {
        Self { request_id, result }
    }

    #[must_use]
    pub const fn is_ok(&self) -> bool {
        matches!(self.result, FailResultStatus::Ok { .. })
    }
}

impl From<(RequestId, app_database::api::requests::FailResult)> for FailResult {
    fn from((request_id, value): (RequestId, app_database::api::requests::FailResult)) -> Self {
        Self::new(request_id, value.into())
    }
}

impl From<FailResult> for super::CentralMessage {
    fn from(value: FailResult) -> Self {
        Self::WorkRequestFailResult(value)
    }
}

impl From<(RequestId, app_database::api::requests::FailResult)> for super::CentralMessage {
    fn from(value: (RequestId, app_database::api::requests::FailResult)) -> Self {
        let value: FailResult = value.into();

        value.into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FailResultStatus {
    RequestNotFound,
    RequestNotInProgress,
    RequestNotTakenByYou,
    Ok {
        reason: String,
        requester_id: Arc<str>,
    },
}

impl From<app_database::api::requests::FailResult> for FailResultStatus {
    fn from(value: app_database::api::requests::FailResult) -> Self {
        match value {
            app_database::api::requests::FailResult::RequestNotFound => Self::RequestNotFound,
            app_database::api::requests::FailResult::RequestNotInProgress => {
                Self::RequestNotInProgress
            }
            app_database::api::requests::FailResult::RequestNotTakenByYou => {
                Self::RequestNotTakenByYou
            }
            app_database::api::requests::FailResult::Ok {
                reason,
                requester_id,
            } => Self::Ok {
                reason,
                requester_id,
            },
        }
    }
}
