use std::sync::Arc;

use app_config::common::ProgramPathConfig;
use tracing::error;

use crate::{cmd::_config::CmdConfig, peering::rpc::RpcClient};

pub mod _common;
pub mod _config;
pub mod discord;
pub mod telegram;

pub async fn run(cfg: CmdConfig, rpc: Arc<RpcClient>, dependency_paths: Arc<ProgramPathConfig>) {
    if let Err(e) = cfg.run(rpc, dependency_paths).await {
        error!(%e, "Failed to run command");
        std::process::exit(1);
    }
}

pub type CmdErr = Box<dyn std::error::Error + Send + Sync>;
pub type CmdResult = Result<(), CmdErr>;

impl CmdConfig {
    async fn run(self, rpc: Arc<RpcClient>, dependency_paths: Arc<ProgramPathConfig>) -> CmdResult {
        match self {
            Self::Telegram(cfg) => telegram::run(*cfg, rpc, dependency_paths).await,
            Self::Discord(cfg) => discord::run(*cfg, rpc).await,
        }
    }
}
