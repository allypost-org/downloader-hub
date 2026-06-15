use std::sync::{Arc, OnceLock};

use app_config::common;
use clap::Args;
use serde::{Deserialize, Serialize};
use validator::Validate;

static JWT_SECRET: OnceLock<Arc<str>> = OnceLock::new();

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

    pub fn init_jwt_secret(secret: Arc<str>) {
        _ = JWT_SECRET.set(secret);
    }

    pub fn jwt_secret() -> Arc<str> {
        JWT_SECRET
            .get()
            .expect("JWT secret not initialized")
            .clone()
    }
}
