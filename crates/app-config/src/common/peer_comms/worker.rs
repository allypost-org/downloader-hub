use std::sync::Arc;

use clap::Args;
use serde::{Deserialize, Serialize};
use url::Url;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
#[clap(next_help_heading = Some("Peer communication worker config"))]
pub struct PeerCommsWorkerConfig {
    #[clap(flatten)]
    #[validate(nested)]
    pub common: super::PeerCommsCommonConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub ticket: PeerCommsWorkerTicketConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
pub struct PeerCommsWorkerTicketConfig {
    #[clap(flatten)]
    #[validate(nested)]
    pub ticket: Option<PeerCommsWorkerTicketFromTicket>,

    #[clap(flatten)]
    #[validate(nested)]
    pub api: Option<PeerCommsWorkerTicketFromApiConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
#[group(requires_all=["ticket"])]
pub struct PeerCommsWorkerTicketFromTicket {
    /// Ticket to join central.
    #[clap(
        long = "peer-comms-worker-ticket",
        env = "DOWNLOADER_HUB_PEER_COMMS_WORKER_TICKET",
        required = false
    )]
    pub ticket: String,

    /// JWT token to authenticate with central.
    #[clap(
        long = "peer-comms-worker-jwt-token",
        env = "DOWNLOADER_HUB_PEER_COMMS_WORKER_JWT_TOKEN",
        required = false
    )]
    pub jwt_token: Option<Arc<str>>,
}

#[derive(derive_more::Debug, Clone, Serialize, Deserialize, Args, Validate)]
#[group(requires_all=["url", "key"])]
pub struct PeerCommsWorkerTicketFromApiConfig {
    /// Central API URL
    #[clap(
        long = "peer-comms-worker-api-url",
        env = "DOWNLOADER_HUB_PEER_COMMS_WORKER_API_URL",
        required = false
    )]
    #[debug("{:?}", url.as_str())]
    pub url: Url,

    /// API key for authentication
    #[clap(
        long = "peer-comms-worker-api-key",
        env = "DOWNLOADER_HUB_PEER_COMMS_WORKER_API_KEY",
        required = false
    )]
    pub key: Arc<str>,
}
