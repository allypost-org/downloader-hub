use clap::Subcommand;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", tag = "$cmd")]
pub enum CmdConfig {
    Run(Box<super::central::config::CentralConfig>),
}

impl Validate for CmdConfig {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        match self {
            Self::Run(cfg) => cfg.validate(),
        }
    }
}

impl CmdConfig {
    pub fn resolve_paths(mut self) -> Self {
        match &mut self {
            Self::Run(cfg) => {
                cfg.with_resolved_paths();
            }
        }

        self
    }
}
