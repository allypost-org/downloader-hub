use serde::{Deserialize, Serialize};

use crate::message::v1::common::RequestId;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatusMessageResult {
    pub request_id: RequestId,
    pub result: UpdateStatusMessageResultStatus,
}

impl UpdateStatusMessageResult {
    #[must_use]
    const fn new(request_id: RequestId, result: UpdateStatusMessageResultStatus) -> Self {
        Self { request_id, result }
    }

    #[must_use]
    pub const fn is_ok(&self) -> bool {
        matches!(self.result, UpdateStatusMessageResultStatus::Ok)
    }
}

impl
    From<(
        RequestId,
        app_database::api::requests::UpdateStatusMessageResult,
    )> for UpdateStatusMessageResult
{
    fn from(
        (request_id, value): (
            RequestId,
            app_database::api::requests::UpdateStatusMessageResult,
        ),
    ) -> Self {
        Self::new(request_id, value.into())
    }
}

impl From<UpdateStatusMessageResult> for super::CentralMessage {
    fn from(value: UpdateStatusMessageResult) -> Self {
        Self::WorkRequestUpdateStatusMessageResult(value)
    }
}

impl
    From<(
        RequestId,
        app_database::api::requests::UpdateStatusMessageResult,
    )> for super::CentralMessage
{
    fn from(
        value: (
            RequestId,
            app_database::api::requests::UpdateStatusMessageResult,
        ),
    ) -> Self {
        let value: UpdateStatusMessageResult = value.into();

        value.into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateStatusMessageResultStatus {
    RequestNotFound,
    RequestNotInProgress,
    RequestNotTakenByYou,
    Ok,
}

impl From<app_database::api::requests::UpdateStatusMessageResult>
    for UpdateStatusMessageResultStatus
{
    fn from(value: app_database::api::requests::UpdateStatusMessageResult) -> Self {
        match value {
            app_database::api::requests::UpdateStatusMessageResult::RequestNotFound => {
                Self::RequestNotFound
            }
            app_database::api::requests::UpdateStatusMessageResult::RequestNotInProgress => {
                Self::RequestNotInProgress
            }
            app_database::api::requests::UpdateStatusMessageResult::RequestNotTakenByYou => {
                Self::RequestNotTakenByYou
            }
            app_database::api::requests::UpdateStatusMessageResult::Ok => Self::Ok,
        }
    }
}
