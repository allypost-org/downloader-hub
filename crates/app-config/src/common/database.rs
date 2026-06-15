use clap::Args;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// Configuration to connect to the Convex database.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Args, Validate)]
#[clap(next_help_heading = "Database config")]
pub struct DatabaseConfig {
    #[arg(long, env = "DOWNLOADER_HUB_DATABASE_URL")]
    #[validate(url)]
    pub database_url: String,
}
