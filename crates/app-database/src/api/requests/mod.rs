use std::{collections::HashMap, convert::Into, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{
    Database, DatabaseError, DatabaseRequest,
    api::accounts::{place_ref_value, user_ref_value},
    entity::{
        accounts::{AccountPlaceRef, AccountUserRef, Platform},
        requests::{file_reference::FileReference, request_info::RequestInfo},
    },
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
        ordered_by: Option<AccountUserRef>,
        ordered_in: Option<AccountPlaceRef>,
    ) -> Result<RequestIdResponse, DatabaseError>
    where
        T: Into<RequestInfo>,
    {
        self.requests_add_with_work_kind(
            requester_id,
            info,
            metadata,
            idempotency_key,
            ordered_by,
            ordered_in,
            None,
        )
        .await
    }

    pub async fn requests_add_with_work_kind<T>(
        &self,
        requester_id: Arc<str>,
        info: T,
        metadata: HashMap<String, String>,
        idempotency_key: Option<String>,
        ordered_by: Option<AccountUserRef>,
        ordered_in: Option<AccountPlaceRef>,
        work_kind: Option<crate::entity::requests::request_info::WorkKind>,
    ) -> Result<RequestIdResponse, DatabaseError>
    where
        T: Into<RequestInfo>,
    {
        let info = info.into();
        let work_kind = work_kind.or_else(|| match info.work_kind() {
            crate::entity::requests::request_info::WorkKind::AccountRefresh => {
                Some(crate::entity::requests::request_info::WorkKind::AccountRefresh)
            }
            crate::entity::requests::request_info::WorkKind::Download => None,
        });

        let mut req = DatabaseRequest::named("requests:add")
            .with_arg(
                "info",
                serde_json::to_string(&info).map_err(DatabaseError::SerializeToString)?,
            )
            .with_arg("requesterId", requester_id.as_ref())
            .with_arg(
                "metadata",
                convex::Value::Object(metadata.into_iter().map(|(k, v)| (k, v.into())).collect()),
            )
            .with_arg("idempotencyKey", idempotency_key);
        if let Some(work_kind) = work_kind {
            req = req.with_arg("workKind", work_kind.as_str());
        }
        if let Some(ordered_by) = ordered_by {
            req = req.with_arg("orderedBy", user_ref_value(&ordered_by));
        }
        if let Some(ordered_in) = ordered_in {
            req = req.with_arg("orderedIn", place_ref_value(&ordered_in));
        }
        req.mutate(self).await
    }
}

#[derive(derive_more::Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestInfoResponse {
    #[serde(rename = "requestId", alias = "_id")]
    pub request_id: Arc<str>,
    pub requester: Arc<str>,
    #[serde(deserialize_with = "RequestInfo::deserialize_db")]
    pub info: RequestInfo,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    pub status: RequestStatus,
    #[serde(default)]
    pub errors: Arc<[Arc<str>]>,
    #[serde(rename = "refusedBy", default)]
    pub refused_by: Arc<[Arc<str>]>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
    #[serde(with = "crate::helpers::serde::bigint")]
    pub last_modified: u64,
    #[serde(default)]
    pub created_at: f64,
    #[serde(default)]
    pub ordered_by: Option<AccountUserRef>,
    #[serde(default)]
    pub ordered_in: Option<AccountPlaceRef>,
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
    Delivering {
        #[serde(with = "crate::helpers::serde::bigint")]
        since: u64,
        #[serde(with = "crate::helpers::serde::bigint")]
        worker_since: u64,
        worker_by: String,
        claimed_by: String,
        delivery_attempt_id: String,
        message: Option<String>,
        files_data: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Done {
        #[serde(with = "crate::helpers::serde::bigint")]
        at: u64,
        by: String,
        #[serde(default)]
        delivered_by: Option<String>,
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
    pub async fn requests_get_all_available(
        &self,
    ) -> Result<Arc<[RequestInfoResponse]>, DatabaseError> {
        DatabaseRequest::named("requests:getAllAvailable")
            .query(self)
            .await
    }

    pub async fn requests_get_available_account_refresh(
        &self,
        platform: Platform,
    ) -> Result<Arc<[RequestInfoResponse]>, DatabaseError> {
        DatabaseRequest::named("requests:getAvailableAccountRefresh")
            .with_arg("platform", platform.as_str())
            .query(self)
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

    pub async fn requests_get_mine_by_id(
        &self,
        request_id: Arc<str>,
        requester_id: Arc<str>,
    ) -> Result<Option<RequestInfoResponse>, DatabaseError> {
        DatabaseRequest::named("requests:getMineById")
            .with_arg("requestId", request_id.as_ref())
            .with_arg("requesterId", requester_id.as_ref())
            .query(self)
            .await
    }

    pub async fn requests_watch_mine_by_id(
        &self,
        request_id: Arc<str>,
        requester_id: Arc<str>,
    ) -> Result<
        impl futures::stream::Stream<Item = Result<Option<RequestInfoResponse>, ResponseError>>,
        DatabaseError,
    > {
        DatabaseRequest::named("requests:getMineById")
            .with_arg("requestId", request_id.as_ref())
            .with_arg("requesterId", requester_id.as_ref())
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
pub enum RefuseResult {
    RequestNotFound,
    RequestNotInProgress,
    RequestNotTakenByYou,
    Ok,
}
impl Database {
    pub async fn requests_refuse(
        &self,
        request_id: Arc<str>,
        taker_id: Arc<str>,
    ) -> Result<RefuseResult, DatabaseError> {
        DatabaseRequest::named("requests:refuse")
            .with_arg("requestId", request_id.as_ref())
            .with_arg("takerId", taker_id.as_ref())
            .mutate(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum ReleaseResult {
    RequestNotFound,
    RequestNotInProgress,
    RequestNotTakenByYou,
    Ok,
}
impl Database {
    pub async fn requests_release(
        &self,
        request_id: Arc<str>,
        taker_id: Arc<str>,
    ) -> Result<ReleaseResult, DatabaseError> {
        DatabaseRequest::named("requests:release")
            .with_arg("requestId", request_id.as_ref())
            .with_arg("takerId", taker_id.as_ref())
            .mutate(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum ClearRefusalsResult {
    RequestNotFound,
    Ok,
}
impl Database {
    pub async fn requests_clear_refusals(
        &self,
        request_id: Arc<str>,
    ) -> Result<ClearRefusalsResult, DatabaseError> {
        DatabaseRequest::named("requests:clearRefusals")
            .with_arg("requestId", request_id.as_ref())
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum CompleteAccountRefreshResult {
    RequestNotFound,
    RequestNotInProgress,
    RequestNotTakenByYou,
    WrongWorkKind,
    Ok,
}
impl Database {
    pub async fn requests_complete_account_refresh(
        &self,
        request_id: Arc<str>,
        taker_id: Arc<str>,
    ) -> Result<CompleteAccountRefreshResult, DatabaseError> {
        DatabaseRequest::named("requests:completeAccountRefresh")
            .with_arg("requestId", request_id.as_ref())
            .with_arg("takerId", taker_id.as_ref())
            .mutate(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum AckDeliveryResult {
    RequestNotFound,
    RequestNotSubmittedByYou,
    NotWaitingForRequester,
    AlreadyDelivering,
    #[serde(rename_all = "camelCase")]
    Claimed {
        delivery_attempt_id: String,
        #[serde(default)]
        files_data: Option<String>,
    },
}
impl Database {
    pub async fn requests_ack_delivery(
        &self,
        request_id: Arc<str>,
        requester_id: Arc<str>,
    ) -> Result<AckDeliveryResult, DatabaseError> {
        DatabaseRequest::named("requests:ackDelivery")
            .with_arg("requestId", request_id.as_ref())
            .with_arg("requesterId", requester_id.as_ref())
            .mutate(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum FinishDeliveryResult {
    RequestNotFound,
    RequestNotSubmittedByYou,
    NotDelivering,
    StaleAttempt,
    Ok,
}
impl Database {
    pub async fn requests_finish_delivery(
        &self,
        request_id: Arc<str>,
        requester_id: Arc<str>,
        delivery_attempt_id: Arc<str>,
    ) -> Result<FinishDeliveryResult, DatabaseError> {
        DatabaseRequest::named("requests:finishDelivery")
            .with_arg("requestId", request_id.as_ref())
            .with_arg("requesterId", requester_id.as_ref())
            .with_arg("deliveryAttemptId", delivery_attempt_id.as_ref())
            .mutate(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum FailDeliveryResult {
    RequestNotFound,
    RequestNotSubmittedByYou,
    NotDelivering,
    StaleAttempt,
    Failed,
}
impl Database {
    pub async fn requests_fail_delivery(
        &self,
        request_id: Arc<str>,
        requester_id: Arc<str>,
        delivery_attempt_id: Arc<str>,
        reason: &str,
    ) -> Result<FailDeliveryResult, DatabaseError> {
        DatabaseRequest::named("requests:failDelivery")
            .with_arg("requestId", request_id.as_ref())
            .with_arg("requesterId", requester_id.as_ref())
            .with_arg("deliveryAttemptId", delivery_attempt_id.as_ref())
            .with_arg("reason", reason)
            .mutate(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum ReleaseDeliveryResult {
    RequestNotFound,
    RequestNotSubmittedByYou,
    NotDelivering,
    StaleAttempt,
    Released,
}
impl Database {
    pub async fn requests_release_delivery(
        &self,
        request_id: Arc<str>,
        requester_id: Arc<str>,
        delivery_attempt_id: Arc<str>,
    ) -> Result<ReleaseDeliveryResult, DatabaseError> {
        DatabaseRequest::named("requests:releaseDelivery")
            .with_arg("requestId", request_id.as_ref())
            .with_arg("requesterId", requester_id.as_ref())
            .with_arg("deliveryAttemptId", delivery_attempt_id.as_ref())
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

/// Paginated response from `requests:getByStatus`. The frontend walks pages
/// via `continue_cursor`; `is_done` becomes true once the end is reached.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestsByStatusPage {
    pub page: Arc<[RequestInfoResponse]>,
    pub is_done: bool,
    pub continue_cursor: Arc<str>,
}

impl Database {
    pub async fn requests_get_by_status(
        &self,
        status_type: RequestStatusType,
        limit: Option<i64>,
        cursor: Option<Arc<str>>,
    ) -> Result<RequestsByStatusPage, DatabaseError> {
        let req = status_by_status_request(status_type, limit, cursor);
        req.query(self).await
    }

    pub async fn requests_get_by_ordered_by(
        &self,
        platform: crate::entity::accounts::Platform,
        id: &str,
        status_type: Option<RequestStatusType>,
        limit: Option<i64>,
        cursor: Option<Arc<str>>,
    ) -> Result<RequestsByStatusPage, DatabaseError> {
        let req = by_ordered_by_request(platform, id, status_type, limit, cursor);
        req.query(self).await
    }

    pub async fn requests_get_by_ordered_in(
        &self,
        platform: crate::entity::accounts::Platform,
        id: &str,
        status_type: Option<RequestStatusType>,
        limit: Option<i64>,
        cursor: Option<Arc<str>>,
    ) -> Result<RequestsByStatusPage, DatabaseError> {
        let req = by_ordered_in_request(platform, id, status_type, limit, cursor);
        req.query(self).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestRequestChange {
    /// Null when there are no requests yet.
    #[serde(default, with = "crate::helpers::serde::bigint::option")]
    pub last_modified: Option<u64>,
}

impl Database {
    /// Watch the latest `lastModified` across all requests. Used by the admin
    /// live-stream as a "request data changed" ping. Emits `null` when there
    /// are no requests.
    pub async fn requests_watch_latest_change(
        &self,
    ) -> Result<
        impl futures::stream::Stream<Item = Result<LatestRequestChange, ResponseError>>,
        DatabaseError,
    > {
        DatabaseRequest::named("requests:getLatestChange")
            .watch_query(self)
            .await
    }

    pub async fn requests_start_backfill_ordered_refs(
        &self,
    ) -> Result<BackfillStarted, DatabaseError> {
        DatabaseRequest::named("requests:startBackfillOrderedRefs")
            .mutate(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackfillStarted {
    pub started: bool,
}

fn status_by_status_request(
    status_type: RequestStatusType,
    limit: Option<i64>,
    cursor: Option<Arc<str>>,
) -> DatabaseRequest {
    let mut req =
        DatabaseRequest::named("requests:getByStatus").with_arg("statusType", status_type.as_str());
    if let Some(limit) = limit
        && limit > 0
    {
        req = req.with_arg("limit", limit);
    }
    if let Some(cursor) = cursor {
        req = req.with_arg("cursor", cursor.as_ref());
    }
    req
}

fn by_ordered_by_request(
    platform: crate::entity::accounts::Platform,
    id: &str,
    status_type: Option<RequestStatusType>,
    limit: Option<i64>,
    cursor: Option<Arc<str>>,
) -> DatabaseRequest {
    let mut req = DatabaseRequest::named("requests:getByOrderedBy")
        .with_arg("platform", platform.as_str())
        .with_arg("id", id);
    if let Some(status_type) = status_type {
        req = req.with_arg("statusType", status_type.as_str());
    }
    if let Some(limit) = limit
        && limit > 0
    {
        req = req.with_arg("limit", limit);
    }
    if let Some(cursor) = cursor {
        req = req.with_arg("cursor", cursor.as_ref());
    }
    req
}

fn by_ordered_in_request(
    platform: crate::entity::accounts::Platform,
    id: &str,
    status_type: Option<RequestStatusType>,
    limit: Option<i64>,
    cursor: Option<Arc<str>>,
) -> DatabaseRequest {
    let mut req = DatabaseRequest::named("requests:getByOrderedIn")
        .with_arg("platform", platform.as_str())
        .with_arg("id", id);
    if let Some(status_type) = status_type {
        req = req.with_arg("statusType", status_type.as_str());
    }
    if let Some(limit) = limit
        && limit > 0
    {
        req = req.with_arg("limit", limit);
    }
    if let Some(cursor) = cursor {
        req = req.with_arg("cursor", cursor.as_ref());
    }
    req
}

#[derive(Debug, Clone, Copy)]
pub enum RequestStatusType {
    Pending,
    InProgress,
    Delivering,
    Done,
    Failed,
}

impl RequestStatusType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "inProgress",
            Self::Delivering => "delivering",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestCounts {
    #[serde(with = "crate::helpers::serde::bigint")]
    pub pending: u64,
    #[serde(with = "crate::helpers::serde::bigint")]
    pub in_progress: u64,
    #[serde(with = "crate::helpers::serde::bigint")]
    pub delivering: u64,
    #[serde(with = "crate::helpers::serde::bigint")]
    pub done: u64,
    #[serde(with = "crate::helpers::serde::bigint")]
    pub failed: u64,
}

impl Database {
    pub async fn requests_counts(&self) -> Result<RequestCounts, DatabaseError> {
        DatabaseRequest::named("requests:counts").query(self).await
    }

    pub async fn requests_watch_counts(
        &self,
    ) -> Result<
        impl futures::stream::Stream<Item = Result<RequestCounts, ResponseError>>,
        DatabaseError,
    > {
        DatabaseRequest::named("requests:counts")
            .watch_query(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum RetryResult {
    RequestNotFound,
    RequestNotRetryable,
    Ok,
}
impl Database {
    pub async fn requests_retry(&self, request_id: Arc<str>) -> Result<RetryResult, DatabaseError> {
        DatabaseRequest::named("requests:retry")
            .with_arg("requestId", request_id.as_ref())
            .mutate(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum CancelResult {
    RequestNotFound,
    Ok,
}
impl Database {
    pub async fn requests_cancel(
        &self,
        request_id: Arc<str>,
        by: Arc<str>,
    ) -> Result<CancelResult, DatabaseError> {
        DatabaseRequest::named("requests:cancel")
            .with_arg("requestId", request_id.as_ref())
            .with_arg("by", by.as_ref())
            .mutate(self)
            .await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code")]
pub enum RemoveResult {
    RequestNotFound,
    Ok,
}
impl Database {
    pub async fn requests_remove(
        &self,
        request_id: Arc<str>,
    ) -> Result<RemoveResult, DatabaseError> {
        DatabaseRequest::named("requests:remove")
            .with_arg("requestId", request_id.as_ref())
            .mutate(self)
            .await
    }
}
