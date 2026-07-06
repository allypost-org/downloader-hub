use std::{net::IpAddr, sync::Arc};

use app_config::common::{DatabaseConfig, PeerCommsAdminConfig};
use clap::Args;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
pub struct AdminConfig {
    #[clap(flatten)]
    #[validate(nested)]
    pub database: DatabaseConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub http: AdminHttpConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub central: PeerCommsAdminConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
#[clap(next_help_heading = Some("Admin HTTP config"))]
pub struct AdminHttpConfig {
    /// Host to bind the admin HTTP server to.
    #[clap(
        long = "admin-http-host",
        env = "DOWNLOADER_HUB_ADMIN_HTTP_HOST",
        default_value = "0.0.0.0"
    )]
    pub http_host: IpAddr,

    /// Port to bind the admin HTTP server to.
    #[clap(
        long = "admin-http-port",
        env = "DOWNLOADER_HUB_ADMIN_HTTP_PORT",
        default_value = "8082"
    )]
    pub http_port: u16,

    /// Secret used to sign admin session cookies (HMAC-SHA256). Any sufficiently
    /// long random string; must be at least 32 bytes (256 bits).
    #[clap(
        long = "admin-session-secret",
        env = "DOWNLOADER_HUB_ADMIN_SESSION_SECRET"
    )]
    #[validate(length(
        min = 32,
        message = "admin-session-secret must be at least 32 bytes (256 bits) of random data"
    ))]
    pub session_secret: Arc<str>,
}

impl AdminHttpConfig {
    #[must_use]
    pub const fn bind_addr(&self) -> std::net::SocketAddr {
        std::net::SocketAddr::new(self.http_host, self.http_port)
    }
}
