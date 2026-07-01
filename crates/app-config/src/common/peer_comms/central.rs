use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use clap::Args;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
#[clap(next_help_heading = Some("Peer communication central config"))]
pub struct PeerCommsCentralConfig {
    #[clap(flatten)]
    #[validate(nested)]
    pub common: super::PeerCommsCommonConfig,

    /// Topic to use for gossip.
    #[clap(long = "peer-comms-central-topic", env = "DOWNLOADER_HUB_PEER_COMMS_CENTRAL_TOPIC", value_parser = super::parse_slice_u8_32)]
    pub topic_id: Option<[u8; 32]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
pub struct WorkerHttpApiConfig {
    /// Host to bind the worker HTTP API to.
    #[clap(
        long = "peer-comms-central-worker-api-host",
        env = "DOWNLOADER_HUB_PEER_COMMS_WORKER_API_HOST",
        default_value = "0.0.0.0"
    )]
    pub api_host: IpAddr,

    /// Port to bind the worker HTTP API to.
    #[clap(
        long = "peer-comms-central-worker-api-port",
        env = "DOWNLOADER_HUB_PEER_COMMS_WORKER_API_PORT",
        default_value = "8080"
    )]
    pub api_port: u16,

    /// Which method to use to extract the client IP address from the request.
    #[clap(
        long = "peer-comms-central-worker-api-request-ip-source",
        env = "DOWNLOADER_HUB_PEER_COMMS_WORKER_API_REQUEST_IP_SOURCE",
        default_value = "RightmostXForwardedFor"
    )]
    pub request_ip_source: String,

    /// JWT secret to use for authentication.
    #[clap(
        long = "peer-comms-central-worker-api-jwt-secret",
        env = "DOWNLOADER_HUB_PEER_COMMS_WORKER_API_JWT_SECRET"
    )]
    pub jwt_secret: Arc<str>,
}
impl WorkerHttpApiConfig {
    #[must_use]
    pub const fn bind_addr(&self) -> SocketAddr {
        SocketAddr::new(self.api_host, self.api_port)
    }
}
