use std::{collections::HashMap, sync::Arc};

use app_config::common::PeerCommsBotTicketFromApiConfig;
use app_database::entity::accounts::{AccountPlaceRef, AccountUserRef, Platform};
use app_peer_comms::{
    IrohEndpointAddr, PeeringEndpoint, irpc, irpc_iroh,
    message::v1::{
        central::{
            ack_delivery_result::WorkRequestAckResult, add_errors_result::AddErrorsResult,
            complete_account_refresh_result::CompleteAccountRefreshResult,
            create_result::CreateResult, fail_delivery_result::WorkRequestFailDeliveryResult,
            fail_result::FailResult, finish_delivery_result::WorkRequestFinishDeliveryResult,
            finish_result::FinishResult,
            get_account_refresh_item_result::GetAccountRefreshItemResult,
            release_delivery_result::WorkRequestReleaseDeliveryResult, take_result::FreeResult,
            work_request_snapshot::WorkRequestSnapshot,
            work_request_watch_event::WorkRequestWatchEvent,
        },
        common::request_info::RequestInfo,
    },
    rpc::{AuthResult, CentralProtocol, RPC_ALPN, request},
};
use arc_swap::ArcSwapOption;

use crate::peering::reconnect::ReconnectCoordinator;

pub struct RpcClient {
    inner: ArcSwapOption<irpc::Client<CentralProtocol>>,
    api_key: Arc<str>,
    capabilities: request::Capabilities,
    peering: Arc<PeeringEndpoint>,
    pub(in crate::peering) api: PeerCommsBotTicketFromApiConfig,
    pub(in crate::peering) coordinator: ReconnectCoordinator,
}

impl RpcClient {
    pub async fn connect(
        peering: Arc<PeeringEndpoint>,
        api: PeerCommsBotTicketFromApiConfig,
        central_addr: IrohEndpointAddr,
        capabilities: request::Capabilities,
    ) -> Result<Arc<Self>, Box<dyn std::error::Error + Send + Sync>> {
        let api_key = api.key.clone();
        let client = Arc::new(
            Self::connect_and_auth(&peering, &api_key, central_addr, &capabilities).await?,
        );

        Ok(Arc::new(Self {
            inner: ArcSwapOption::from(Some(client)),
            api_key,
            capabilities,
            peering,
            api,
            coordinator: ReconnectCoordinator::default(),
        }))
    }

    /// Re-establish the authenticated irpc session against the given central
    /// address. Auth is connection-scoped, so a new QUIC connection must re-`Auth`
    /// before any call - otherwise central closes it with `unauthenticated`.
    ///
    /// `central_addr` is re-resolved by the caller (`Self::reconnect`), since
    /// central's `NodeId` is NOT assumed stable (the key may be unpinned, or another
    /// node may take over).
    pub async fn reauth(
        &self,
        central_addr: IrohEndpointAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = Arc::new(
            Self::connect_and_auth(
                &self.peering,
                &self.api_key,
                central_addr,
                &self.capabilities,
            )
            .await?,
        );
        self.inner.store(Some(client));
        Ok(())
    }

    async fn connect_and_auth(
        peering: &PeeringEndpoint,
        api_key: &Arc<str>,
        central_addr: IrohEndpointAddr,
        capabilities: &request::Capabilities,
    ) -> Result<irpc::Client<CentralProtocol>, Box<dyn std::error::Error + Send + Sync>> {
        let endpoint = peering.router.endpoint().clone();
        let client = irpc_iroh::client::<CentralProtocol>(endpoint, central_addr, RPC_ALPN);

        match client
            .rpc(request::Auth {
                api_key: api_key.clone(),
                capabilities: capabilities.clone(),
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

        Ok(client)
    }

    #[must_use]
    pub fn peering(&self) -> Arc<PeeringEndpoint> {
        self.peering.clone()
    }

    fn client(&self) -> Arc<irpc::Client<CentralProtocol>> {
        self.inner
            .load_full()
            .expect("downloader-bot RPC client not connected")
    }
}

impl RpcClient {
    pub async fn work_request_create<T>(
        &self,
        info: T,
        metadata: HashMap<String, String>,
        idempotency_key: Option<String>,
        ordered_by: Option<AccountUserRef>,
        ordered_in: Option<AccountPlaceRef>,
    ) -> Result<CreateResult, irpc::Error>
    where
        T: Into<RequestInfo>,
    {
        let req = request::WorkRequestMake {
            info: info.into(),
            metadata,
            idempotency_key,
            ordered_by,
            ordered_in,
        };

        match self.client().rpc(req.clone()).await {
            Ok(res) => Ok(res),
            Err(e) => {
                tracing::warn!(
                    ?e,
                    "work_request_create failed; reconnecting and retrying once"
                );
                if let Err(re) = self.reconnect().await {
                    tracing::warn!(?re, "reconnect failed during work_request_create retry");
                    return Err(e);
                }
                self.client().rpc(req).await
            }
        }
    }

    pub async fn accounts_upsert(
        &self,
        users: Vec<app_database::entity::accounts::AccountUser>,
        places: Vec<app_database::entity::accounts::AccountPlace>,
    ) -> Result<request::AccountsUpsertResult, irpc::Error> {
        self.client()
            .rpc(request::AccountsUpsert { users, places })
            .await
    }

    pub async fn get_account_refresh_item(
        &self,
        platform: Platform,
    ) -> Result<GetAccountRefreshItemResult, irpc::Error> {
        self.client()
            .rpc(request::GetAccountRefreshItem {
                platform: platform.as_str().to_string(),
            })
            .await
    }

    pub async fn complete_account_refresh(
        &self,
        request_id: Arc<str>,
    ) -> Result<CompleteAccountRefreshResult, irpc::Error> {
        self.client()
            .rpc(request::CompleteAccountRefresh { request_id })
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

    pub async fn work_request_fail(
        &self,
        request_id: Arc<str>,
        reason: Arc<str>,
    ) -> Result<FailResult, irpc::Error> {
        self.client()
            .rpc(request::WorkRequestFail { request_id, reason })
            .await
    }

    pub async fn work_request_free(&self, request_id: Arc<str>) -> Result<FreeResult, irpc::Error> {
        self.client()
            .rpc(request::WorkRequestFree { request_id })
            .await
    }

    /// Legacy complete RPC. Retained alongside the `WorkRequestComplete`
    /// protocol variant until the snapshot protocol is explicitly removed.
    /// New delivery paths use `work_request_finish_delivery` instead.
    #[allow(dead_code)]
    pub async fn work_request_complete(
        &self,
        request_id: Arc<str>,
    ) -> Result<FinishResult, irpc::Error> {
        self.client()
            .rpc(request::WorkRequestComplete { request_id })
            .await
    }

    /// Server-streaming per-request watch. Returns a receiver of
    /// `WorkRequestWatchEvent`. `watch_id` is generated by the caller for
    /// logging; cancellation relies on the irpc channel (dropping the
    /// receiver signals central's sender, which closes the watch task).
    pub async fn work_request_wait(
        &self,
        request_id: Arc<str>,
        watch_id: u64,
    ) -> Result<irpc::channel::mpsc::Receiver<WorkRequestWatchEvent>, irpc::Error> {
        self.client()
            .server_streaming(
                request::WorkRequestWait {
                    request_id,
                    watch_id,
                },
                16,
            )
            .await
    }

    pub async fn work_request_ack(
        &self,
        request_id: Arc<str>,
    ) -> Result<WorkRequestAckResult, irpc::Error> {
        self.client()
            .rpc(request::WorkRequestAck { request_id })
            .await
    }

    pub async fn work_request_finish_delivery(
        &self,
        request_id: Arc<str>,
        delivery_attempt_id: Arc<str>,
    ) -> Result<WorkRequestFinishDeliveryResult, irpc::Error> {
        self.client()
            .rpc(request::WorkRequestFinishDelivery {
                request_id,
                delivery_attempt_id,
            })
            .await
    }

    pub async fn work_request_release_delivery(
        &self,
        request_id: Arc<str>,
        delivery_attempt_id: Arc<str>,
    ) -> Result<WorkRequestReleaseDeliveryResult, irpc::Error> {
        self.client()
            .rpc(request::WorkRequestReleaseDelivery {
                request_id,
                delivery_attempt_id,
            })
            .await
    }

    pub async fn work_request_fail_delivery(
        &self,
        request_id: Arc<str>,
        delivery_attempt_id: Arc<str>,
        reason: Arc<str>,
    ) -> Result<WorkRequestFailDeliveryResult, irpc::Error> {
        self.client()
            .rpc(request::WorkRequestFailDelivery {
                request_id,
                delivery_attempt_id,
                reason,
            })
            .await
    }

    pub async fn work_request_list_mine_in_progress(
        &self,
    ) -> Result<WorkRequestSnapshot, irpc::Error> {
        self.client()
            .rpc(request::WorkRequestListMineInProgress)
            .await
    }

    /// Legacy full-snapshot watch. Retained alongside the
    /// `WorkRequestGetMineInProgress` protocol variant until the snapshot
    /// protocol is explicitly removed. New code uses per-request
    /// `work_request_wait` instead.
    #[allow(dead_code)]
    pub async fn work_request_watch_mine_in_progress(
        &self,
    ) -> Result<irpc::channel::mpsc::Receiver<WorkRequestSnapshot>, irpc::Error> {
        self.client()
            .server_streaming(request::WorkRequestGetMineInProgress, 16)
            .await
    }

    pub async fn heartbeat(&self) -> Result<(), irpc::Error> {
        self.client().rpc(request::Heartbeat).await
    }

    pub async fn get_log_settings(&self) -> Result<request::LogSettingsResult, irpc::Error> {
        self.client().rpc(request::GetLogSettings).await
    }

    pub async fn get_capabilities(
        &self,
    ) -> Result<app_peer_comms::rpc::request::CapabilitiesSummary, irpc::Error> {
        self.client().rpc(request::GetCapabilities).await
    }
}
