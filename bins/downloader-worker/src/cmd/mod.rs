use _config::CmdConfig;
use tracing::error;

pub mod _config;
pub mod list;
pub mod work;

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
            Self::List(cfg) => list::run(*cfg),
            Self::Run(cfg) => work::run(*cfg),
        }
    }
}
