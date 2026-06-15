use std::sync::OnceLock;

use app_peer_comms::PeeringEndpoint;
use serde::{Deserialize, Serialize};
use tracing::trace;

use crate::peering::jwt::JwtPair;

static RPC_CLIENT: OnceLock<RpcClient> = OnceLock::new();

pub mod broadcast;
pub mod work_request;

#[derive(Debug)]
pub struct RpcClient {
    rpc_url: url::Url,
}

impl RpcClient {
    pub fn init(base_url: &url::Url) {
        _ = RPC_CLIENT.set(Self {
            rpc_url: base_url.join("/api/rpc").expect("Invalid base URL"),
        });
    }

    pub fn global() -> &'static Self {
        RPC_CLIENT.get().expect("RPC client not initialized")
    }

    pub fn rpc_url() -> &'static url::Url {
        &Self::global().rpc_url
    }

    async fn auth_bearer() -> String {
        format!("Bearer {}", JwtPair::token().await)
    }
}

impl RpcClient {
    pub async fn request_v1<T>(msg: T) -> Result<RpcResponse, RpcClientError>
    where
        T: Into<app_peer_comms::message::v1::V1Message>,
    {
        Self::request(msg.into()).await
    }

    pub async fn request<T, R>(msg: T) -> Result<R, RpcClientError>
    where
        T: Into<app_peer_comms::Message>,
        R: serde::de::DeserializeOwned,
    {
        let msg = msg.into();

        trace!(target: PeeringEndpoint::trace_span_name(), ?msg, "Sending RPC request");

        let resp = reqwest::Client::builder()
            .build()
            .map_err(RpcClientError::BuildClient)?
            .post(Self::rpc_url().clone())
            .header("Authorization", Self::auth_bearer().await)
            .header("Accept", "application/postcard, application/json")
            .json(&msg)
            .send()
            .await
            .map_err(RpcClientError::SendRequest)?
            .error_for_status()
            .map_err(RpcClientError::ErrorStatus)?;

        let response_type = resp
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/json")
            .to_string();

        let resp_body = resp.bytes().await.map_err(RpcClientError::ReadResponse)?;

        match response_type.as_str() {
            "application/postcard" => {
                let resp_body = postcard::from_bytes(&resp_body)
                    .map_err(RpcClientError::ParsePostcardResponse)?;
                Ok(resp_body)
            }
            "application/json" => {
                let resp_body = serde_json::from_slice(&resp_body)
                    .map_err(RpcClientError::ParseJsonResponse)?;
                Ok(resp_body)
            }
            t => Err(RpcClientError::UnhandledResponseType(t.to_string())),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RpcClientError {
    #[error("Failed to build client: {0}")]
    BuildClient(reqwest::Error),

    #[error("Failed to send request: {0}")]
    SendRequest(reqwest::Error),

    #[error("Request failed with status code: {0}")]
    ErrorStatus(reqwest::Error),

    #[error("Failed to read response: {0}")]
    ReadResponse(reqwest::Error),

    #[error("Got error response: {0}")]
    ErrorResponse(String),

    #[error("Unhandled response type: {0}")]
    UnhandledResponseType(String),

    #[error("Failed to parse JSON response: {0}")]
    ParseJsonResponse(serde_json::Error),

    #[error("Failed to parse postcard response: {0}")]
    ParsePostcardResponse(postcard::Error),

    #[error("Failed to connect to the WebSocket: {0}")]
    TungsteniteError(tokio_tungstenite::tungstenite::Error),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RpcResponse {
    Data(Option<app_peer_comms::Message>),
    Error(String),
}
