use std::sync::Arc;

use clap::Args;
use serde::{Deserialize, Serialize};
use url::Url;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
#[clap(next_help_heading = Some("Peer communication bot config"))]
pub struct PeerCommsBotConfig {
    #[clap(flatten)]
    #[validate(nested)]
    pub common: super::PeerCommsCommonConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub ticket: PeerCommsBotTicketConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
pub struct PeerCommsBotTicketConfig {
    #[clap(flatten)]
    #[validate(nested)]
    pub ticket: Option<PeerCommsBotTicketFromTicket>,

    #[clap(flatten)]
    #[validate(nested)]
    pub api: PeerCommsBotTicketFromApiConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
#[group(requires_all=["ticket"])]
pub struct PeerCommsBotTicketFromTicket {
    /// Ticket to join central.
    #[clap(
        long = "peer-comms-bot-ticket",
        env = "DOWNLOADER_HUB_PEER_COMMS_BOT_TICKET",
        required = false
    )]
    pub ticket: String,

    /// JWT token to authenticate with central.
    #[clap(
        long = "peer-comms-bot-jwt-token",
        env = "DOWNLOADER_HUB_PEER_COMMS_BOT_JWT_TOKEN",
        required = false
    )]
    pub jwt_token: Option<Arc<str>>,
}

#[derive(derive_more::Debug, Clone, Serialize, Deserialize, Args, Validate)]
#[group(requires_all=["url", "key"])]
pub struct PeerCommsBotTicketFromApiConfig {
    /// Central API URL
    #[clap(
        long = "peer-comms-bot-api-url",
        env = "DOWNLOADER_HUB_PEER_COMMS_BOT_API_URL",
        required = false
    )]
    #[debug("{:?}", url.as_str())]
    pub url: Url,

    /// API key for authentication
    #[clap(
        long = "peer-comms-bot-api-key",
        env = "DOWNLOADER_HUB_PEER_COMMS_BOT_API_KEY",
        required = false
    )]
    pub key: Arc<str>,
}
