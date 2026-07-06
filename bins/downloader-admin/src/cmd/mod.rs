pub mod _config;
pub mod run;

pub use _config::CmdConfig;

pub type CmdErr = Box<dyn std::error::Error + Send + Sync>;
pub type CmdResult = Result<(), CmdErr>;

pub fn run(cfg: CmdConfig) {
    cfg.run_top();
}
