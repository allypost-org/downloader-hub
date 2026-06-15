use std::{collections::HashMap, convert::Into, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{
    Database, DatabaseError, DatabaseRequest,
    entity::requests::{file_reference::FileReference, request_info::RequestInfo},
    error::ResponseError,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestIdResponse {
    #[serde(rename = "requestId")]
    pub id: Arc<str>,
}
impl Database {
    pub async fn requests_add<T>(
        &self,
        requester_id: Arc<str>,
        info: T,
        metadata: HashMap<String, String>,
        idempotency_key: Option<String>,
    ) -> Result<RequestIdResponse, DatabaseError>
    where
        T: Into<RequestInfo>,
    {
        let info = info.into();

        DatabaseRequest::named("requests:add")
            .with_arg(
                "info",
                serde_json::to_string(&info).map_err(DatabaseError::SerializeToString)?,
            )
            .with_arg("requesterId", requester_id.as_ref())
            .with_arg(
                "metadata",
                convex::Value::Object(metadata.into_iter().map(|(k, v)| (k, v.into())).collect()),
            )
            .with_arg("idempotencyKey", idempotency_key)
            .mutate(self)
            .await
    }
}

#[derive(derive_more::Debug, Clone, Serialize, Deserialize)]
pub struct RequestInfoResponse {
    #[serde(rename = "requestId", alias = "_id")]
    pub request_id: Arc<str>,
    #[serde(deserialize_with = "RequestInfo::deserialize_db")]
    pub info: RequestInfo,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    pub status: RequestStatus,
    #[serde(default)]
    pub errors: Arc<[Arc<str>]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "Type")]
pub enum RequestStatus {
    Pending,
    #[serde(rename_all = "camelCase")]
    InProgress {
        #[serde(with = "crate::helpers::serde::bigint")]
        since: u64,
        by: String,
        message: Option<String>,
        waiting_for_requester: Option<bool>,
        files_data: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Done {
        #[serde(with = "crate::helpers::serde::bigint")]
        at: u64,
        by: String,
    },
    #[serde(rename_all = "camelCase")]
    Failed {
        #[serde(with = "crate::helpers::serde::bigint")]
        at: u64,
        by: String,
        reason: String,
    },
}

impl Database {
    pub async fn requests_get_first_available(
        &self,
    ) -> Result<Option<RequestInfoResponse>, DatabaseError> {
        DatabaseRequest::named("requests:getFirstAvailable")
            .query(self)
            .await
    }

    pub async fn requests_watch_first_available(
        &self,
    ) -> Result<
        impl futures::stream::Stream<Item = Result<Option<RequestInfoResponse>, ResponseError>>,
        DatabaseError,
    > {
        DatabaseRequest::named("requests:getFirstAvailable")
            .watch_query(self)
            .await
    }
}

impl Database {
    pub async fn requests_watch_all_available(
        &self,
    ) -> Result<
        impl futures::stream::Stream<Item = Result<Arc<[RequestInfoResponse]>, ResponseError>>,
        DatabaseError,
    > {
        DatabaseRequest::named("requests:getAllAvailable")
            .watch_query(self)
            .await
    }
}

impl Database {
    pub async fn requests_get(
        &self,
        request_id: Arc<str>,
    ) -> Result<RequestInfoResponse, DatabaseError> {
        DatabaseRequest::named("requests:get")
            .with_arg("requestId", request_id.as_ref())
            .query(self)
            .await
    }

    pub async fn requests_watch(
        &self,
        request_id: Arc<str>,
    ) -> Result<
        impl futures::stream::Stream<Item = Result<Option<RequestInfoResponse>, ResponseError>>,
        DatabaseError,
    > {
        DatabaseRequest::named("requests:get")
            .with_arg("requestId", request_id.as_ref())
            .watch_query(self)
            .await
    }

    pub async fn requests_watch_mine_in_progress(
        &self,
        authed_id: Arc<str>,
    ) -> Result<
        impl futures::stream::Stream<Item = Result<Arc<[RequestInfoResponse]>, ResponseError>>,
        DatabaseError,
    > {
        DatabaseRequest::named("requests:getMineInProgress")
            .with_arg("authedId", authed_id.as_ref())
            .watch_query(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum TakeResult {
    RequestNotFound,
    RequestAlreadyTaken,
    RequestNotInProgress,
    RequestNotTakenByYou,
    Ok(Box<RequestInfoResponse>),
}
impl Database {
    pub async fn requests_take(
        &self,
        request_id: Arc<str>,
        taker_id: Arc<str>,
    ) -> Result<TakeResult, DatabaseError> {
        DatabaseRequest::named("requests:take")
            .with_arg("requestId", request_id.as_ref())
            .with_arg("takerId", taker_id.as_ref())
            .mutate(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum FreeResult {
    RequestNotFound,
    RequestNotInProgress,
    RequestNotTakenByYou,
    Ok,
}
impl Database {
    pub async fn requests_free(
        &self,
        request_id: Arc<str>,
        taker_id: Arc<str>,
    ) -> Result<FreeResult, DatabaseError> {
        DatabaseRequest::named("requests:free")
            .with_arg("requestId", request_id.as_ref())
            .with_arg("takerId", taker_id.as_ref())
            .mutate(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum UpdateStatusMessageResult {
    RequestNotFound,
    RequestNotInProgress,
    RequestNotTakenByYou,
    Ok,
}
impl Database {
    pub async fn requests_update_status_message(
        &self,
        request_id: Arc<str>,
        authed_id: Arc<str>,
        message: &str,
    ) -> Result<UpdateStatusMessageResult, DatabaseError> {
        DatabaseRequest::named("requests:updateStatusMessage")
            .with_arg("requestId", request_id.as_ref())
            .with_arg("authedId", authed_id.as_ref())
            .with_arg("statusMessage", message)
            .mutate(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum AddErrorsResult {
    RequestNotFound,
    RequestNotInProgress,
    RequestNotTakenByYou,
    Ok,
}
impl Database {
    pub async fn requests_add_errors(
        &self,
        request_id: Arc<str>,
        authed_id: Arc<str>,
        errors: Vec<String>,
    ) -> Result<AddErrorsResult, DatabaseError> {
        DatabaseRequest::named("requests:addErrors")
            .with_arg("requestId", request_id.as_ref())
            .with_arg("authedId", authed_id.as_ref())
            .with_arg(
                "errors",
                errors
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<convex::Value>>(),
            )
            .mutate(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum MoveToWaitingForRequesterResult {
    RequestNotFound,
    RequestNotInProgress,
    RequestNotTakenByYou,
    Ok,
}
impl Database {
    pub async fn requests_move_to_waiting_for_requester(
        &self,
        request_id: Arc<str>,
        authed_id: Arc<str>,
        files_data: Vec<FileReference>,
    ) -> Result<MoveToWaitingForRequesterResult, DatabaseError> {
        DatabaseRequest::named("requests:moveToWaitingForRequester")
            .with_arg("requestId", request_id.as_ref())
            .with_arg("authedId", authed_id.as_ref())
            .with_arg(
                "filesData",
                serde_json::to_string(&files_data).map_err(DatabaseError::SerializeToString)?,
            )
            .mutate(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum FailResult {
    RequestNotFound,
    RequestNotInProgress,
    RequestNotTakenByYou,
    #[serde(rename_all = "camelCase")]
    Ok {
        requester_id: Arc<str>,
        reason: String,
    },
}
impl Database {
    pub async fn requests_fail(
        &self,
        request_id: Arc<str>,
        taker_id: Arc<str>,
        reason: &str,
    ) -> Result<FailResult, DatabaseError> {
        DatabaseRequest::named("requests:fail")
            .with_arg("requestId", request_id.as_ref())
            .with_arg("authedId", taker_id.as_ref())
            .with_arg("reason", reason)
            .mutate(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum FinishResult {
    RequestNotFound,
    RequestNotInProgress,
    RequestNotSubmittedByYou,
    Ok,
}
impl Database {
    pub async fn requests_finish(
        &self,
        request_id: Arc<str>,
        requester_id: Arc<str>,
    ) -> Result<FinishResult, DatabaseError> {
        DatabaseRequest::named("requests:finish")
            .with_arg("requestId", request_id.as_ref())
            .with_arg("requesterId", requester_id.as_ref())
            .mutate(self)
            .await
    }
}

impl Database {
    pub async fn requests_get_mine_in_progress(
        &self,
        authed_id: Arc<str>,
    ) -> Result<Arc<[RequestInfoResponse]>, DatabaseError> {
        DatabaseRequest::named("requests:getMineInProgress")
            .with_arg("authedId", authed_id.as_ref())
            .query(self)
            .await
    }
}
