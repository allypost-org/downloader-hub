use app_config::common;
use clap::Args;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
pub struct CentralConfig {
    #[clap(flatten)]
    #[validate(nested)]
    pub database: common::DatabaseConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub peer: common::PeerCommsCentralConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub worker_api: common::WorkerHttpApiConfig,
}

impl CentralConfig {
    pub const fn with_resolved_paths(&mut self) -> &mut Self {
        self
    }
}
