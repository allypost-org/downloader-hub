use serde::{Deserialize, Serialize};

use crate::message::v1::common::RequestId;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveToWaitingForRequesterResult {
    pub request_id: RequestId,
    pub result: MoveToWaitingForRequesterResultStatus,
}

impl MoveToWaitingForRequesterResult {
    #[must_use]
    const fn new(request_id: RequestId, result: MoveToWaitingForRequesterResultStatus) -> Self {
        Self { request_id, result }
    }

    #[must_use]
    pub const fn is_ok(&self) -> bool {
        matches!(self.result, MoveToWaitingForRequesterResultStatus::Ok)
    }
}

impl
    From<(
        RequestId,
        app_database::api::requests::MoveToWaitingForRequesterResult,
    )> for MoveToWaitingForRequesterResult
{
    fn from(
        (request_id, value): (
            RequestId,
            app_database::api::requests::MoveToWaitingForRequesterResult,
        ),
    ) -> Self {
        Self::new(request_id, value.into())
    }
}

impl From<MoveToWaitingForRequesterResult> for super::CentralMessage {
    fn from(value: MoveToWaitingForRequesterResult) -> Self {
        Self::WorkRequestMoveToWaitingForRequesterResult(value)
    }
}

impl
    From<(
        RequestId,
        app_database::api::requests::MoveToWaitingForRequesterResult,
    )> for super::CentralMessage
{
    fn from(
        value: (
            RequestId,
            app_database::api::requests::MoveToWaitingForRequesterResult,
        ),
    ) -> Self {
        let value: MoveToWaitingForRequesterResult = value.into();

        value.into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MoveToWaitingForRequesterResultStatus {
    RequestNotFound,
    RequestNotInProgress,
    RequestNotTakenByYou,
    Ok,
}

impl From<app_database::api::requests::MoveToWaitingForRequesterResult>
    for MoveToWaitingForRequesterResultStatus
{
    fn from(value: app_database::api::requests::MoveToWaitingForRequesterResult) -> Self {
        match value {
            app_database::api::requests::MoveToWaitingForRequesterResult::RequestNotFound => {
                Self::RequestNotFound
            }
            app_database::api::requests::MoveToWaitingForRequesterResult::RequestNotInProgress => {
                Self::RequestNotInProgress
            }
            app_database::api::requests::MoveToWaitingForRequesterResult::RequestNotTakenByYou => {
                Self::RequestNotTakenByYou
            }
            app_database::api::requests::MoveToWaitingForRequesterResult::Ok => Self::Ok,
        }
    }
}
