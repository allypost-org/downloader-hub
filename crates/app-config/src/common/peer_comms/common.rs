use std::path::PathBuf;

use clap::Args;
use serde::{Deserialize, Serialize};
use url::Url;
use validator::Validate;

pub static DEFAULT_TOPIC_ID_VERSION: u8 = 1;
pub static DEFAULT_TOPIC_ID: [u8; 32] = [
    DEFAULT_TOPIC_ID_VERSION,
    45,
    68,
    79,
    87,
    78,
    76,
    79,
    65,
    68,
    69,
    82,
    45,
    72,
    85,
    66,
    45,
    80,
    69,
    69,
    82,
    45,
    67,
    79,
    77,
    77,
    83,
    // ^ $VERSION-DOWNLOADER-HUB-PEER-COMMS
    // v random bytes
    0,
    0,
    0,
    0,
    0,
];

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
pub struct PeerCommsCommonConfig {
    /// Secret key to derive our node id from.
    #[clap(
        long = "peer-comms-secret-key",
        env = "DOWNLOADER_HUB_PEER_COMMS_SECRET_KEY",
        value_parser = super::parse_slice_u8_32
    )]
    pub secret_key: Option<[u8; 32]>,

    #[clap(flatten)]
    #[validate(nested)]
    pub relay: RelayOptions,

    #[clap(flatten)]
    #[validate(nested)]
    pub bind: BindConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub blob: BlobConfig,
}

#[derive(derive_more::Debug, Clone, Serialize, Deserialize, Args, Validate)]
#[group(multiple = false)]
pub struct RelayOptions {
    /// Set a custom relay server. By default, the relay server hosted by n0 will be used.
    #[clap(
        long = "peer-comms-relay",
        env = "DOWNLOADER_HUB_PEER_COMMS_RELAY",
        value_delimiter = ','
    )]
    #[debug("{:?}", relays.iter().map(std::string::ToString::to_string).collect::<Vec<_>>())]
    pub relays: Vec<Url>,

    /// Disable relay completely.
    #[clap(
        long = "peer-comms-no-relay",
        env = "DOWNLOADER_HUB_PEER_COMMS_NO_RELAY"
    )]
    pub no_relay: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
pub struct BindConfig {
    /// Set the bind port for our both sockets. By default, a random port will be used.
    #[clap(
        long = "peer-comms-bind-port",
        env = "DOWNLOADER_HUB_PEER_COMMS_BIND_PORT",
        default_value = "0"
    )]
    pub port: u16,

    /// Set the bind port for our IPv4 socket. By default, a random port will be used.
    #[clap(
        long = "peer-comms-bind-port-v4",
        env = "DOWNLOADER_HUB_PEER_COMMS_BIND_PORT_V4"
    )]
    pub port_v4: Option<u16>,

    /// Set the IPv4 bind address for our socket. By default, all interfaces will be used.
    #[clap(
        long = "peer-comms-bind-addr-v4",
        env = "DOWNLOADER_HUB_PEER_COMMS_BIND_ADDR_V4",
        default_value = "0.0.0.0"
    )]
    pub addr_v4: std::net::Ipv4Addr,

    /// Set the bind port for our IPv6 socket. By default, a random port will be used.
    #[clap(
        long = "peer-comms-bind-port-v6",
        env = "DOWNLOADER_HUB_PEER_COMMS_BIND_PORT_V6"
    )]
    pub port_v6: Option<u16>,

    /// Set the IPv6 bind address for our socket. By default, all interfaces will be used.
    #[clap(
        long = "peer-comms-bind-addr-v6",
        env = "DOWNLOADER_HUB_PEER_COMMS_BIND_ADDR_V6",
        default_value = "::"
    )]
    pub addr_v6: std::net::Ipv6Addr,
}

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
pub struct BlobConfig {
    /// Store path for blobs.
    #[clap(
        long = "peer-comms-blob-store-path",
        env = "DOWNLOADER_HUB_PEER_COMMS_BLOB_STORE_PATH"
    )]
    pub store: Option<PathBuf>,
}
