use irpc::{
    channel::{mpsc, oneshot},
    rpc_requests,
};
use serde::{Deserialize, Serialize};

use crate::message::v1::central::{
    ack_delivery_result::WorkRequestAckResult, add_errors_result::AddErrorsResult,
    create_result::CreateResult, fail_delivery_result::WorkRequestFailDeliveryResult,
    fail_result::FailResult, finish_delivery_result::WorkRequestFinishDeliveryResult,
    finish_result::FinishResult, get_work_item_result::GetWorkItemResult,
    move_to_waiting_for_requester_result::MoveToWaitingForRequesterResult,
    release_delivery_result::WorkRequestReleaseDeliveryResult, take_result::FreeResult,
    update_status_message_result::UpdateStatusMessageResult,
    work_request_snapshot::WorkRequestSnapshot, work_request_watch_event::WorkRequestWatchEvent,
};

pub mod auth_result;
pub mod request;
pub mod session;

pub use auth_result::AuthResult;
pub use session::{AuthedInfo, Role, Session};

pub const RPC_ALPN: &[u8] = b"downloader-hub/rpc/1";

#[rpc_requests(message = CentralRequest)]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CentralProtocol {
    #[rpc(tx = oneshot::Sender<AuthResult>)]
    Auth(request::Auth),

    #[rpc(tx = oneshot::Sender<()>)]
    Heartbeat(request::Heartbeat),

    #[rpc(tx = oneshot::Sender<GetWorkItemResult>)]
    GetWorkItem(request::GetWorkItem),
    #[rpc(tx = oneshot::Sender<FreeResult>)]
    RefuseWorkItem(request::RefuseWorkItem),
    #[rpc(tx = oneshot::Sender<FreeResult>)]
    WorkRequestFree(request::WorkRequestFree),
    #[rpc(tx = oneshot::Sender<UpdateStatusMessageResult>)]
    WorkRequestUpdateStatus(request::WorkRequestUpdateStatus),
    #[rpc(tx = oneshot::Sender<AddErrorsResult>)]
    WorkRequestAddErrors(request::WorkRequestAddErrors),
    #[rpc(tx = oneshot::Sender<MoveToWaitingForRequesterResult>)]
    WorkRequestMoveToWaiting(request::WorkRequestMoveToWaiting),
    #[rpc(tx = oneshot::Sender<FailResult>)]
    WorkRequestFail(request::WorkRequestFail),

    #[rpc(tx = oneshot::Sender<CreateResult>)]
    WorkRequestMake(request::WorkRequestMake),
    #[rpc(tx = oneshot::Sender<FinishResult>)]
    WorkRequestComplete(request::WorkRequestComplete),
    #[rpc(tx = mpsc::Sender<WorkRequestSnapshot>)]
    WorkRequestGetMineInProgress(request::WorkRequestGetMineInProgress),

    #[rpc(tx = oneshot::Sender<request::CapabilitiesSummary>)]
    GetCapabilities(request::GetCapabilities),

    #[rpc(tx = oneshot::Sender<request::AdminSessionsResult>)]
    AdminListSessions(request::AdminListSessions),
    #[rpc(tx = oneshot::Sender<request::AdminParkedWorkersResult>)]
    AdminListParkedWorkers(request::AdminListParkedWorkers),

    #[rpc(tx = oneshot::Sender<request::AccountsUpsertResult>)]
    AccountsUpsert(request::AccountsUpsert),

    #[rpc(tx = mpsc::Sender<WorkRequestWatchEvent>)]
    WorkRequestWait(request::WorkRequestWait),
    #[rpc(tx = oneshot::Sender<WorkRequestAckResult>)]
    WorkRequestAck(request::WorkRequestAck),
    #[rpc(tx = oneshot::Sender<WorkRequestFinishDeliveryResult>)]
    WorkRequestFinishDelivery(request::WorkRequestFinishDelivery),
    #[rpc(tx = oneshot::Sender<WorkRequestReleaseDeliveryResult>)]
    WorkRequestReleaseDelivery(request::WorkRequestReleaseDelivery),
    #[rpc(tx = oneshot::Sender<WorkRequestSnapshot>)]
    WorkRequestListMineInProgress(request::WorkRequestListMineInProgress),
    #[rpc(tx = oneshot::Sender<WorkRequestFailDeliveryResult>)]
    WorkRequestFailDelivery(request::WorkRequestFailDelivery),
}
