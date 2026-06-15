use app_config::common;
use clap::Args;
use serde::{Deserialize, Serialize};
use validator::Validate;

/// Listen for new requests
#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
pub struct WorkerConfig {
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
    pub peer: common::PeerCommsWorkerConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub task: common::TaskConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub request: common::RequestConfig,
}

impl WorkerConfig {
    pub fn with_resolved_paths(&mut self) -> &mut Self {
        self.dependency_paths.with_resolved_paths();
        self
    }
}
