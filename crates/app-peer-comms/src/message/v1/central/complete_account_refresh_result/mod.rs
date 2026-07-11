use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompleteAccountRefreshResult {
    RequestNotFound,
    RequestNotInProgress,
    RequestNotTakenByYou,
    WrongWorkKind,
    Ok,
    BackendError,
    Unauthorized,
}

impl CompleteAccountRefreshResult {
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }
}

impl From<app_database::api::requests::CompleteAccountRefreshResult>
    for CompleteAccountRefreshResult
{
    fn from(value: app_database::api::requests::CompleteAccountRefreshResult) -> Self {
        match value {
            app_database::api::requests::CompleteAccountRefreshResult::RequestNotFound => {
                Self::RequestNotFound
            }
            app_database::api::requests::CompleteAccountRefreshResult::RequestNotInProgress => {
                Self::RequestNotInProgress
            }
            app_database::api::requests::CompleteAccountRefreshResult::RequestNotTakenByYou => {
                Self::RequestNotTakenByYou
            }
            app_database::api::requests::CompleteAccountRefreshResult::WrongWorkKind => {
                Self::WrongWorkKind
            }
            app_database::api::requests::CompleteAccountRefreshResult::Ok => Self::Ok,
        }
    }
}
