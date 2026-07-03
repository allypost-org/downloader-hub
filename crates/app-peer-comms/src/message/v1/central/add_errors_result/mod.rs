use serde::{Deserialize, Serialize};

use crate::message::v1::common::RequestId;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddErrorsResult {
    pub request_id: RequestId,
    pub result: AddErrorsResultStatus,
}

impl AddErrorsResult {
    #[must_use]
    const fn new(request_id: RequestId, result: AddErrorsResultStatus) -> Self {
        Self { request_id, result }
    }

    #[must_use]
    pub const fn is_ok(&self) -> bool {
        matches!(self.result, AddErrorsResultStatus::Ok)
    }
}

impl From<(RequestId, app_database::api::requests::AddErrorsResult)> for AddErrorsResult {
    fn from(
        (request_id, value): (RequestId, app_database::api::requests::AddErrorsResult),
    ) -> Self {
        Self::new(request_id, value.into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AddErrorsResultStatus {
    RequestNotFound,
    RequestNotInProgress,
    RequestNotTakenByYou,
    Ok,
    BackendError,
    Unauthorized,
}

impl From<app_database::api::requests::AddErrorsResult> for AddErrorsResultStatus {
    fn from(value: app_database::api::requests::AddErrorsResult) -> Self {
        match value {
            app_database::api::requests::AddErrorsResult::RequestNotFound => Self::RequestNotFound,
            app_database::api::requests::AddErrorsResult::RequestNotInProgress => {
                Self::RequestNotInProgress
            }
            app_database::api::requests::AddErrorsResult::RequestNotTakenByYou => {
                Self::RequestNotTakenByYou
            }
            app_database::api::requests::AddErrorsResult::Ok => Self::Ok,
        }
    }
}
