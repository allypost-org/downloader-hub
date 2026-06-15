use clap::{Args, ValueHint};
use serde::{Deserialize, Serialize};
use url::Url;
use validator::Validate;

use crate::validators::{
    str::value_parser_ensure_min_length,
    url::{validate_url_is_absolute_url, value_parser_parse_absolute_url},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, Args, Validate)]
pub struct ServerConfig {
    #[clap(flatten)]
    #[validate(nested)]
    pub run: ServerRunConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub database: DatabaseConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub app: AppConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Args, Validate)]
#[clap(next_help_heading = "Server options")]
pub struct ServerRunConfig {
    /// The port on which the server will listen.
    #[arg(long, default_value = "8000", env = "PORT", value_parser = clap::value_parser!(u16).range(1..))]
    pub port: u16,

    /// The host on which the server will listen.
    #[arg(long, default_value = "127.0.0.1", env = "HOST")]
    pub host: String,

    /// The admin key for the server.
    /// Used to authenticate admin requests.
    /// Should be at least 32 characters long and securely random.
    #[arg(long, env = "DOWNLOADER_HUB_ADMIN_KEY", value_parser = value_parser_ensure_min_length(32))]
    #[validate(length(min = 32))]
    pub admin_key: String,

    /// The key used for signing various tokens.
    /// Should be at least 32 characters long and securely random.
    #[arg(long, env = "DOWNLOADER_HUB_SIGNING_KEY", value_parser = value_parser_ensure_min_length(32))]
    #[validate(length(min = 32))]
    pub signing_key: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Args, Validate)]
#[clap(next_help_heading = "Database options")]
pub struct DatabaseConfig {
    /// `PostgreSQL` database URL.
    ///
    /// Should be in the format of `postgres://username:password@db-host:5432/database-name`
    #[clap(long = "database-url", env = "DATABASE_URL")]
    #[validate(url)]
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
#[clap(next_help_heading = "Application options")]
pub struct AppConfig {
    /// The public URL where the application is served.
    /// This is used to generate links to the application.
    /// Should be in the format of `https://www.example.com/some/path` or `http://127.0.0.1:8000`
    #[clap(long, env = "DOWNLOADER_HUB_PUBLIC_URL", value_hint = ValueHint::Url, value_parser = value_parser_parse_absolute_url())]
    #[validate(custom(function = "validate_url_is_absolute_url"))]
    pub public_url: Url,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            public_url: Url::parse("http://127.0.0.1:8000").expect("Invalid URL"),
        }
    }
}
