use app_config::common;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// List available actions, downloaders, or fixers
#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
pub struct ListConfig {
    #[clap(subcommand)]
    #[validate(nested)]
    pub which: CmdList,

    #[clap(flatten)]
    #[validate(nested)]
    pub dependency_paths: common::ProgramPathConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub disabled_entries: common::DisabledEntriesConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub endpoint: common::EndpointConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub request: common::RequestConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Subcommand)]
pub enum CmdList {
    Actions,
    Downloaders,
    Fixers,
    All,
}
impl Validate for CmdList {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        Ok(())
    }
}

impl ListConfig {
    pub fn with_resolved_paths(&mut self) -> &mut Self {
        self.dependency_paths.with_resolved_paths();
        self
    }
}
