use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FinishResult {
    RequestNotFound,
    RequestNotInProgress,
    RequestNotSubmittedByYou,
    Ok,
}

impl FinishResult {
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }
}

impl From<app_database::api::requests::FinishResult> for FinishResult {
    fn from(value: app_database::api::requests::FinishResult) -> Self {
        match value {
            app_database::api::requests::FinishResult::RequestNotFound => Self::RequestNotFound,
            app_database::api::requests::FinishResult::RequestNotInProgress => {
                Self::RequestNotInProgress
            }
            app_database::api::requests::FinishResult::RequestNotSubmittedByYou => {
                Self::RequestNotSubmittedByYou
            }
            app_database::api::requests::FinishResult::Ok => Self::Ok,
        }
    }
}

impl From<FinishResult> for super::CentralMessage {
    fn from(value: FinishResult) -> Self {
        Self::WorkRequestFinishResponse(value)
    }
}

impl From<app_database::api::requests::FinishResult> for super::CentralMessage {
    fn from(value: app_database::api::requests::FinishResult) -> Self {
        let value: FinishResult = value.into();

        value.into()
    }
}
