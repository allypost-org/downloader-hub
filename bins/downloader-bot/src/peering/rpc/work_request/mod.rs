use std::{collections::HashMap, sync::Arc};

use app_peer_comms::{
    PeeringEndpoint,
    message::v1::{
        bot::BotMessage,
        central::{CentralMessage, finish_result::FinishResult},
        common::request_info::RequestInfo,
    },
};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tracing::debug;

use super::{RpcClientError, RpcResponse};
use crate::peering::jwt::JwtPair;

impl super::RpcClient {
    pub async fn work_request_create<T>(
        info: T,
        metadata: HashMap<String, String>,
        idempotency_key: Option<String>,
    ) -> Result<RpcResponse, RpcClientError>
    where
        T: Into<RequestInfo>,
    {
        Self::request_v1(BotMessage::WorkRequestMake {
            info: info.into(),
            metadata,
            idempotency_key,
        })
        .await
    }

    pub async fn work_request_watch_mine_in_progress() -> Result<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        RpcClientError,
    > {
        let url = {
            let mut url = Self::rpc_url()
                .join("/api/v1/watch/work-requests")
                .expect("Invalid URL");
            let scheme = url.scheme();
            let new = if scheme == "https" { "wss" } else { "ws" };
            url.set_scheme(new).expect("Failed to set scheme");
            url
        };

        let request = {
            let mut request = url
                .clone()
                .into_client_request()
                .map_err(RpcClientError::TungsteniteError)?;

            request.headers_mut().insert(
                "Authorization",
                format!("Bearer {}", JwtPair::token().await)
                    .parse()
                    .expect("Failed to parse header"),
            );

            request.headers_mut().append(
                "Accept",
                "application/postcard"
                    .parse()
                    .expect("Failed to parse header"),
            );

            request.headers_mut().append(
                "Accept",
                "application/json".parse().expect("Failed to parse header"),
            );

            request
        };

        debug!(target: PeeringEndpoint::trace_span_name(), url = ?url.as_str(), "Connecting to the status watcher");

        let (conn, _request) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(RpcClientError::TungsteniteError)?;

        Ok(conn)
    }

    pub async fn work_request_complete(
        request_id: Arc<str>,
    ) -> Result<FinishResult, RpcClientError> {
        let resp = Self::request_v1(BotMessage::WorkRequestComplete { request_id }).await?;

        let resp = match resp {
            RpcResponse::Data(data) => data,
            RpcResponse::Error(e) => return Err(RpcClientError::ErrorResponse(e)),
        };

        let Some(resp) = resp else {
            return Err(RpcClientError::ErrorResponse("No response".to_string()));
        };

        let app_peer_comms::Message::V1(app_peer_comms::message::v1::V1Message::Central(
            CentralMessage::WorkRequestFinishResponse(resp),
        )) = resp
        else {
            return Err(RpcClientError::ErrorResponse(
                "Invalid response".to_string(),
            ));
        };

        Ok(resp)
    }
}
