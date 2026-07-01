use clap::Subcommand;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", tag = "$cmd")]
pub enum CmdConfig {
    List(Box<super::list::config::ListConfig>),
    Run(Box<super::work::config::WorkerConfig>),
}

impl Validate for CmdConfig {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        match self {
            Self::List(cfg) => cfg.validate(),
            Self::Run(cfg) => cfg.validate(),
        }
    }
}

impl CmdConfig {
    pub fn resolve_paths(mut self) -> Self {
        match &mut self {
            Self::List(cfg) => {
                cfg.with_resolved_paths();
            }
            Self::Run(cfg) => {
                cfg.with_resolved_paths();
            }
        }

        self
    }
}
