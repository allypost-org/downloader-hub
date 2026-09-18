use std::sync::Arc;

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

impl RpcClient {
    pub async fn connect(
        peering: &PeeringEndpoint,
        api_key: Arc<str>,
        central_addr: IrohEndpointAddr,
        capabilities: request::Capabilities,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let endpoint = peering.router.endpoint().clone();
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

        Ok(Self {
            inner: ArcSwapOption::from(Some(Arc::new(client))),
        })
    }

    fn client(&self) -> Arc<irpc::Client<CentralProtocol>> {
        self.inner
            .load_full()
            .expect("downloader-worker RPC client not connected")
    }

    pub async fn get_work_item(&self) -> Result<GetWorkItemResult, irpc::Error> {
        self.client().rpc(request::GetWorkItem).await
    }

    pub async fn refuse_work_item(&self, request_id: Arc<str>) -> Result<FreeResult, irpc::Error> {
        self.client()
            .rpc(request::RefuseWorkItem { request_id })
            .await
    }

    pub async fn heartbeat(&self) -> Result<(), irpc::Error> {
        self.client().rpc(request::Heartbeat).await
    }

    pub async fn get_log_settings(&self) -> Result<request::LogSettingsResult, irpc::Error> {
        self.client().rpc(request::GetLogSettings).await
    }

    pub async fn get_secrets(&self) -> Result<request::SecretsResult, irpc::Error> {
        self.client().rpc(request::GetSecrets).await
    }

    pub async fn work_request_free(&self, request_id: Arc<str>) -> Result<FreeResult, irpc::Error> {
        self.client()
            .rpc(request::WorkRequestFree { request_id })
            .await
    }

    pub async fn work_request_update_status_message(
        &self,
        request_id: Arc<str>,
        message: Arc<str>,
    ) -> Result<UpdateStatusMessageResult, irpc::Error> {
        self.client()
            .rpc(request::WorkRequestUpdateStatus {
                request_id,
                message,
            })
            .await
    }

    pub async fn work_request_add_errors(
        &self,
        request_id: Arc<str>,
        errors: Vec<String>,
    ) -> Result<AddErrorsResult, irpc::Error> {
        self.client()
            .rpc(request::WorkRequestAddErrors { request_id, errors })
            .await
    }

    pub async fn work_request_move_to_waiting_for_requester(
        &self,
        request_id: Arc<str>,
        files_data: Vec<app_peer_comms::message::v1::common::file::FileReference>,
    ) -> Result<MoveToWaitingForRequesterResult, irpc::Error> {
        self.client()
            .rpc(request::WorkRequestMoveToWaiting {
                request_id,
                files_data,
            })
            .await
    }

    pub async fn work_request_fail(
        &self,
        request_id: Arc<str>,
        reason: Arc<str>,
    ) -> Result<FailResult, irpc::Error> {
        self.client()
            .rpc(request::WorkRequestFail { request_id, reason })
            .await
    }
}
