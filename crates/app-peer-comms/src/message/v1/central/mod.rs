use std::{convert::Into, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{jwt::JwtPair, message::v1::common::RequestId};

pub mod add_errors_result;
pub mod create_result;
pub mod fail_result;
pub mod finish_result;
pub mod move_to_waiting_for_requester_result;
pub mod take_result;
pub mod update_status_message_result;
pub mod work_request;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CentralMessage {
    AcceptAuthentication(JwtPair),
    RejectAuthentication {
        reason: String,
    },
    WorkRequest(Box<work_request::WorkRequest>),
    WorkRequests(Arc<[work_request::WorkRequest]>),
    WorkRequestsTakeResponse(take_result::TakeResult),
    WorkRequestFreed(take_result::FreeResult),
    WorkRequestFailed(fail_result::FailResult),
    WorkRequestCreateResponse(create_result::CreateResult),
    WorkRequestFinishResponse(finish_result::FinishResult),
    WorkRequestUpdateStatusMessageResult(update_status_message_result::UpdateStatusMessageResult),
    WorkRequestAddErrorsResult(add_errors_result::AddErrorsResult),
    WorkRequestMoveToWaitingForRequesterResult(
        move_to_waiting_for_requester_result::MoveToWaitingForRequesterResult,
    ),
    WorkRequestFailResult(fail_result::FailResult),
}

impl CentralMessage {
    pub fn work_request<T>(request: T) -> Self
    where
        T: Into<work_request::WorkRequest>,
    {
        Self::WorkRequest(Box::new(request.into()))
    }

    pub fn work_requests<I, T>(requests: I) -> Result<Self, T::Error>
    where
        I: IntoIterator<Item = T>,
        T: TryInto<work_request::WorkRequest>,
    {
        let reqs = requests
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self::WorkRequests(reqs.into()))
    }
}

impl From<CentralMessage> for super::V1Message {
    fn from(msg: CentralMessage) -> Self {
        Self::Central(msg)
    }
}

impl From<CentralMessage> for super::Message {
    fn from(msg: CentralMessage) -> Self {
        Self::V1(msg.into())
    }
}

impl TryFrom<(RequestId, app_database::api::requests::TakeResult)> for CentralMessage {
    type Error = take_result::TakeResultError;

    fn try_from(
        msg: (RequestId, app_database::api::requests::TakeResult),
    ) -> Result<Self, Self::Error> {
        Ok(Self::WorkRequestsTakeResponse(msg.try_into()?))
    }
}

impl From<(RequestId, app_database::api::requests::FreeResult)> for CentralMessage {
    fn from(msg: (RequestId, app_database::api::requests::FreeResult)) -> Self {
        Self::WorkRequestFreed(msg.into())
    }
}
