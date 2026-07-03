use serde::{Deserialize, Serialize};

use super::work_request::request::WorkRequestError;
use crate::message::v1::common::RequestId;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TakeResult {
    Err(RequestId, TakeResultErrMessage),
    Ok(Box<super::work_request::request::WorkRequest>),
}

impl TakeResult {
    #[must_use]
    pub const fn err_message(&self) -> Option<&'static str> {
        match self {
            Self::Err(_, err) => Some(err.msg()),
            Self::Ok(_) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TakeResultErrMessage {
    RequestNotFound,
    RequestNotInProgress,
    RequestNotTakenByYou,
    RequestAlreadyTaken,
    SomethingWentWrong,
}

impl TakeResultErrMessage {
    #[must_use]
    pub const fn msg(&self) -> &'static str {
        match self {
            Self::RequestNotFound => "Request not found",
            Self::RequestNotInProgress => "Request not in progress",
            Self::RequestNotTakenByYou => "Request not taken by me",
            Self::RequestAlreadyTaken => "Request already taken",
            Self::SomethingWentWrong => "Something went wrong",
        }
    }
}

impl TryFrom<(RequestId, app_database::api::requests::TakeResult)> for TakeResult {
    type Error = TakeResultError;

    fn try_from(
        (request_id, value): (RequestId, app_database::api::requests::TakeResult),
    ) -> Result<Self, Self::Error> {
        match value {
            app_database::api::requests::TakeResult::RequestNotFound => {
                Ok(Self::Err(request_id, TakeResultErrMessage::RequestNotFound))
            }
            app_database::api::requests::TakeResult::RequestNotInProgress => Ok(Self::Err(
                request_id,
                TakeResultErrMessage::RequestNotInProgress,
            )),
            app_database::api::requests::TakeResult::RequestNotTakenByYou => Ok(Self::Err(
                request_id,
                TakeResultErrMessage::RequestNotTakenByYou,
            )),
            app_database::api::requests::TakeResult::RequestAlreadyTaken => Ok(Self::Err(
                request_id,
                TakeResultErrMessage::RequestAlreadyTaken,
            )),
            app_database::api::requests::TakeResult::Ok(data) => {
                Ok(Self::Ok(Box::new(data.as_ref().try_into()?)))
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TakeResultError {
    #[error("Invalid work request: {0}")]
    InvalidWorkRequest(#[from] WorkRequestError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FreeResult {
    RequestNotFound { request_id: RequestId },
    RequestNotInProgress { request_id: RequestId },
    RequestNotTakenByYou { request_id: RequestId },
    Ok { request_id: RequestId },
    BackendError { request_id: RequestId },
    Unauthorized { request_id: RequestId },
}

impl FreeResult {
    #[must_use]
    pub const fn request_id(&self) -> &RequestId {
        match self {
            Self::RequestNotFound { request_id }
            | Self::RequestNotInProgress { request_id }
            | Self::RequestNotTakenByYou { request_id }
            | Self::Ok { request_id }
            | Self::BackendError { request_id }
            | Self::Unauthorized { request_id } => request_id,
        }
    }
}

impl From<(RequestId, app_database::api::requests::FreeResult)> for FreeResult {
    fn from((request_id, value): (RequestId, app_database::api::requests::FreeResult)) -> Self {
        match value {
            app_database::api::requests::FreeResult::RequestNotFound => {
                Self::RequestNotFound { request_id }
            }
            app_database::api::requests::FreeResult::RequestNotInProgress => {
                Self::RequestNotInProgress { request_id }
            }
            app_database::api::requests::FreeResult::RequestNotTakenByYou => {
                Self::RequestNotTakenByYou { request_id }
            }
            app_database::api::requests::FreeResult::Ok => Self::Ok { request_id },
        }
    }
}

impl From<(RequestId, app_database::api::requests::RefuseResult)> for FreeResult {
    fn from((request_id, value): (RequestId, app_database::api::requests::RefuseResult)) -> Self {
        match value {
            app_database::api::requests::RefuseResult::RequestNotFound => {
                Self::RequestNotFound { request_id }
            }
            app_database::api::requests::RefuseResult::RequestNotInProgress => {
                Self::RequestNotInProgress { request_id }
            }
            app_database::api::requests::RefuseResult::RequestNotTakenByYou => {
                Self::RequestNotTakenByYou { request_id }
            }
            app_database::api::requests::RefuseResult::Ok => Self::Ok { request_id },
        }
    }
}
