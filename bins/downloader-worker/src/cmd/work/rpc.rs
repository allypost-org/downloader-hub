use std::sync::{Arc, OnceLock};

use app_peer_comms::{
    IrohEndpointAddr, PeeringEndpoint, irpc, irpc_iroh,
    message::v1::central::{
        add_errors_result::AddErrorsResult, fail_result::FailResult,
        get_work_item_result::GetWorkItemResult,
        move_to_waiting_for_requester_result::MoveToWaitingForRequesterResult,
        take_result::FreeResult, update_status_message_result::UpdateStatusMessageResult,
    },
    rpc::{AuthResult, CentralProtocol, RPC_ALPN, request},
};
use arc_swap::ArcSwapOption;

pub struct RpcClient {
    inner: ArcSwapOption<irpc::Client<CentralProtocol>>,
}

static RPC_CLIENT: OnceLock<RpcClient> = OnceLock::new();

impl RpcClient {
    pub async fn init(
        api_key: Arc<str>,
        central_addr: IrohEndpointAddr,
        capabilities: app_peer_comms::rpc::request::Capabilities,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let endpoint = PeeringEndpoint::global().router.endpoint().clone();
        let client = irpc_iroh::client::<CentralProtocol>(endpoint, central_addr, RPC_ALPN);

        match client
            .rpc(request::Auth {
                api_key,
                capabilities,
                version: crate::config::Config::app_version().to_string(),
            })
            .await?
        {
            AuthResult::Ok(info) => {
                tracing::info!(?info, "irpc session established");
            }
            AuthResult::Unauthorized => {
                tracing::error!(
                    "irpc authentication rejected; the API key is likely revoked or expired. \
                     Terminating."
                );
                std::process::exit(1);
            }
        }

        let client = Arc::new(client);
        match RPC_CLIENT.get() {
            Some(existing) => existing.inner.store(Some(client)),
            None => {
                _ = RPC_CLIENT.set(Self {
                    inner: ArcSwapOption::from(Some(client)),
                });
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn global() -> &'static Self {
        RPC_CLIENT
            .get()
            .expect("downloader-worker RPC client not initialized")
    }

    fn client() -> Arc<irpc::Client<CentralProtocol>> {
        Self::global()
            .inner
            .load_full()
            .expect("downloader-worker RPC client not initialized")
    }
}

impl RpcClient {
    pub async fn get_work_item() -> Result<GetWorkItemResult, irpc::Error> {
        Self::client().rpc(request::GetWorkItem).await
    }

    pub async fn refuse_work_item(request_id: Arc<str>) -> Result<FreeResult, irpc::Error> {
        Self::client()
            .rpc(request::RefuseWorkItem { request_id })
            .await
    }

    pub async fn heartbeat() -> Result<(), irpc::Error> {
        Self::client().rpc(request::Heartbeat).await
    }

    pub async fn work_request_free(request_id: Arc<str>) -> Result<FreeResult, irpc::Error> {
        Self::client()
            .rpc(request::WorkRequestFree { request_id })
            .await
    }

    pub async fn work_request_update_status_message(
        request_id: Arc<str>,
        message: Arc<str>,
    ) -> Result<UpdateStatusMessageResult, irpc::Error> {
        Self::client()
            .rpc(request::WorkRequestUpdateStatus {
                request_id,
                message,
            })
            .await
    }

    pub async fn work_request_add_errors(
        request_id: Arc<str>,
        errors: Vec<String>,
    ) -> Result<AddErrorsResult, irpc::Error> {
        Self::client()
            .rpc(request::WorkRequestAddErrors { request_id, errors })
            .await
    }

    pub async fn work_request_move_to_waiting_for_requester(
        request_id: Arc<str>,
        files_data: Vec<app_peer_comms::message::v1::common::file::FileReference>,
    ) -> Result<MoveToWaitingForRequesterResult, irpc::Error> {
        Self::client()
            .rpc(request::WorkRequestMoveToWaiting {
                request_id,
                files_data,
            })
            .await
    }

    pub async fn work_request_fail(
        request_id: Arc<str>,
        reason: Arc<str>,
    ) -> Result<FailResult, irpc::Error> {
        Self::client()
            .rpc(request::WorkRequestFail { request_id, reason })
            .await
    }
}
