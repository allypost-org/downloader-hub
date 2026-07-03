use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};

use app_peer_comms::{
    IrohEndpointAddr, PeeringEndpoint, irpc, irpc_iroh,
    message::v1::{
        central::{
            create_result::CreateResult, finish_result::FinishResult,
            work_request_snapshot::WorkRequestSnapshot,
        },
        common::request_info::RequestInfo,
    },
    rpc::{AuthResult, CentralProtocol, RPC_ALPN, request},
};
use arc_swap::ArcSwapOption;

pub struct RpcClient {
    inner: ArcSwapOption<irpc::Client<CentralProtocol>>,
    api_key: Arc<str>,
    capabilities: request::Capabilities,
}

static RPC_CLIENT: OnceLock<RpcClient> = OnceLock::new();

impl RpcClient {
    pub async fn init(
        api_key: Arc<str>,
        central_addr: IrohEndpointAddr,
        capabilities: request::Capabilities,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = Arc::new(Self::connect_and_auth(&api_key, central_addr, &capabilities).await?);
        match RPC_CLIENT.get() {
            Some(existing) => existing.inner.store(Some(client)),
            None => {
                _ = RPC_CLIENT.set(Self {
                    inner: ArcSwapOption::from(Some(client)),
                    api_key,
                    capabilities,
                });
            }
        }
        Ok(())
    }

    /// Re-establish the authenticated irpc session against the given central
    /// address. Auth is connection-scoped, so a new QUIC connection must re-`Auth`
    /// before any call — otherwise central closes it with `unauthenticated`.
    ///
    /// `central_addr` is re-resolved by the caller (`peering::reconnect`), since
    /// central's `NodeId` is NOT assumed stable (the key may be unpinned, or another
    /// node may take over).
    pub async fn reauth(
        central_addr: IrohEndpointAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let this = Self::global();
        let client = Arc::new(
            Self::connect_and_auth(&this.api_key, central_addr, &this.capabilities).await?,
        );
        this.inner.store(Some(client));
        Ok(())
    }

    async fn connect_and_auth(
        api_key: &Arc<str>,
        central_addr: IrohEndpointAddr,
        capabilities: &request::Capabilities,
    ) -> Result<irpc::Client<CentralProtocol>, Box<dyn std::error::Error + Send + Sync>> {
        let endpoint = PeeringEndpoint::global().router.endpoint().clone();
        let client = irpc_iroh::client::<CentralProtocol>(endpoint, central_addr, RPC_ALPN);

        match client
            .rpc(request::Auth {
                api_key: api_key.clone(),
                capabilities: capabilities.clone(),
                version: crate::config::Config::app_version().to_string(),
            })
            .await?
        {
            AuthResult::Ok => {}
            AuthResult::Unauthorized => return Err("irpc authentication rejected".into()),
        }

        Ok(client)
    }

    #[must_use]
    pub fn global() -> &'static Self {
        RPC_CLIENT
            .get()
            .expect("downloader-bot RPC client not initialized")
    }

    fn client() -> Arc<irpc::Client<CentralProtocol>> {
        Self::global()
            .inner
            .load_full()
            .expect("downloader-bot RPC client not initialized")
    }
}

impl RpcClient {
    pub async fn work_request_create<T>(
        info: T,
        metadata: HashMap<String, String>,
        idempotency_key: Option<String>,
    ) -> Result<CreateResult, irpc::Error>
    where
        T: Into<RequestInfo>,
    {
        Self::client()
            .rpc(request::WorkRequestMake {
                info: info.into(),
                metadata,
                idempotency_key,
            })
            .await
    }

    pub async fn work_request_complete(request_id: Arc<str>) -> Result<FinishResult, irpc::Error> {
        Self::client()
            .rpc(request::WorkRequestComplete { request_id })
            .await
    }

    pub async fn work_request_watch_mine_in_progress()
    -> Result<irpc::channel::mpsc::Receiver<WorkRequestSnapshot>, irpc::Error> {
        Self::client()
            .server_streaming(request::WorkRequestGetMineInProgress, 16)
            .await
    }

    pub async fn heartbeat() -> Result<(), irpc::Error> {
        Self::client().rpc(request::Heartbeat).await
    }

    pub async fn get_capabilities()
    -> Result<app_peer_comms::rpc::request::CapabilitiesSummary, irpc::Error> {
        Self::client().rpc(request::GetCapabilities).await
    }
}
