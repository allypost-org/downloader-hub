use clap::Subcommand;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::cmd::run::config::AdminConfig;

#[derive(Debug, Clone, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", tag = "$cmd")]
pub enum CmdConfig {
    Run(Box<AdminConfig>),
}

impl Validate for CmdConfig {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        match self {
            Self::Run(cfg) => cfg.validate(),
        }
    }
}

impl CmdConfig {
    #[inline]
    fn run(self) -> super::CmdResult {
        match self {
            Self::Run(cfg) => super::run::run(*cfg),
        }
    }

    pub fn run_top(self) {
        if let Err(e) = self.run() {
            tracing::error!(%e, "Failed to run command");
            std::process::exit(1);
        }
    }
}
