use std::sync::Arc;

use clap::Args;
use serde::{Deserialize, Serialize};
use url::Url;
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
#[clap(next_help_heading = Some("Peer communication admin config"))]
pub struct PeerCommsAdminConfig {
    #[clap(flatten)]
    #[validate(nested)]
    pub common: super::PeerCommsCommonConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub api: Option<PeerCommsAdminApiConfig>,
}

#[derive(derive_more::Debug, Clone, Serialize, Deserialize, Args, Validate)]
#[group(requires_all = ["url", "key"])]
pub struct PeerCommsAdminApiConfig {
    /// Central API URL used to fetch the admin join ticket.
    #[clap(
        long = "peer-comms-admin-api-url",
        env = "DOWNLOADER_HUB_PEER_COMMS_ADMIN_API_URL",
        required = false
    )]
    #[debug("{:?}", url.as_str())]
    pub url: Url,

    /// Admin API key used to authenticate with central.
    #[clap(
        long = "peer-comms-admin-api-key",
        env = "DOWNLOADER_HUB_PEER_COMMS_ADMIN_API_KEY",
        required = false
    )]
    pub key: Arc<str>,
}
