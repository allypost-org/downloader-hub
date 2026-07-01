use _config::CmdConfig;
use tracing::error;

pub mod _config;
pub mod central;

pub fn run(cfg: CmdConfig) {
    if let Err(e) = cfg.run() {
        error!(%e, "Failed to run command");
        std::process::exit(1);
    }
}

pub type CmdErr = Box<dyn std::error::Error + Send + Sync>;
pub type CmdResult = Result<(), CmdErr>;

impl CmdConfig {
    fn run(self) -> CmdResult {
        match self {
            Self::Run(cfg) => central::run(*cfg),
        }
    }
}
