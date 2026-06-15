use tracing::error;

use crate::cmd::_config::CmdConfig;

pub mod _config;
pub mod discord;
pub mod telegram;

pub async fn run(cfg: CmdConfig) {
    if let Err(e) = cfg.run().await {
        error!(%e, "Failed to run command");
        std::process::exit(1);
    }
}

pub type CmdErr = Box<dyn std::error::Error + Send + Sync>;
pub type CmdResult = Result<(), CmdErr>;

impl CmdConfig {
    async fn run(self) -> CmdResult {
        match self {
            Self::Telegram(cfg) => telegram::run(*cfg).await,
            Self::Discord(cfg) => discord::run(*cfg).await,
        }
    }
}
