use std::sync::Arc;

use app_config::common::PeerCommsAdminApiConfig;
use app_peer_comms::{
    irpc,
    rpc::{CentralProtocol, request},
};
use serde::Deserialize;

pub struct CentralClient {
    rpc: Arc<irpc::Client<CentralProtocol>>,
    api: PeerCommsAdminApiConfig,
}

impl CentralClient {
    #[must_use]
    pub const fn new(
        rpc: Arc<irpc::Client<CentralProtocol>>,
        api: PeerCommsAdminApiConfig,
    ) -> Self {
        Self { rpc, api }
    }

    pub const fn api(&self) -> &PeerCommsAdminApiConfig {
        &self.api
    }

    pub async fn list_sessions(&self) -> Result<request::AdminSessionsResult, irpc::Error> {
        self.rpc.rpc(request::AdminListSessions).await
    }

    pub async fn list_parked_workers(
        &self,
    ) -> Result<request::AdminParkedWorkersResult, irpc::Error> {
        self.rpc.rpc(request::AdminListParkedWorkers).await
    }

    pub async fn get_capabilities(&self) -> Result<request::CapabilitiesSummary, irpc::Error> {
        self.rpc.rpc(request::GetCapabilities).await
    }

    async fn central_get<T>(&self, path: &str) -> Result<T, CentralProxyError>
    where
        T: serde::de::DeserializeOwned,
    {
        let url = self.api.url.join(path)?;
        let resp = app_requests::Client::builder()
            .build()?
            .get(url)
            .header("Authorization", format!("Bearer {}", self.api.key))
            .send()
            .await?
            .error_for_status()?
            .json::<T>()
            .await?;
        Ok(resp)
    }

    pub async fn proxy_connections(&self) -> Result<Vec<serde_json::Value>, CentralProxyError> {
        #[derive(Debug, Deserialize)]
        struct Resp {
            data: RespData,
        }
        #[derive(Debug, Deserialize)]
        struct RespData {
            connections: Vec<serde_json::Value>,
        }
        let resp: Resp = self.central_get("/api/v1/connections").await?;
        Ok(resp.data.connections)
    }

    pub async fn proxy_metrics_raw(&self) -> Result<String, CentralProxyError> {
        let url = self.api.url.join("/api/v1/metrics")?;
        let text = app_requests::Client::builder()
            .build()?
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        Ok(text)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CentralProxyError {
    #[error("request failed: {0}")]
    Request(#[from] app_requests::reqwest::Error),
    #[error("invalid url: {0}")]
    Url(#[from] url::ParseError),
}
