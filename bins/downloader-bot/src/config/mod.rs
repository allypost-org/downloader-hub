use app_config::{
    BUILD_DATE, BUILD_RUSTC_VERSION, Dumpable, GlobalConfig,
    common::{APPLICATION_NAME, PeerCommsBotConfig, ProgramPathConfig},
    validators::print_validation_errors,
};
use clap::Parser;
use const_format::concatcp;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::cmd::_config::CmdConfig;

pub static APPLICATION_VERSION: &str = env!("CARGO_PKG_VERSION");
pub static APPLICATION_NAME_WITH_VERSION: &str =
    concatcp!(APPLICATION_NAME, " v", APPLICATION_VERSION);

#[derive(Debug, Clone, Serialize, Deserialize, Parser, Validate, GlobalConfig, Dumpable)]
pub struct Config {
    #[clap(subcommand)]
    #[validate(nested)]
    pub cmd: CmdConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub peer: PeerCommsBotConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub dependency_paths: ProgramPathConfig,

    #[clap(flatten)]
    #[validate(nested)]
    #[serde(skip)]
    dump: DumpConfig,
}

impl Config {
    pub fn init_parsed() -> Result<&'static Self, String> {
        let parsed = Self::parse()
            .resolve_paths()
            .validate_or_exit()
            .dump_if_needed();

        Self::init(parsed)
    }

    #[inline]
    fn resolve_paths(mut self) -> Self {
        self.dependency_paths = self.dependency_paths.resolve_paths();
        self
    }

    #[inline]
    fn validate_or_exit(self) -> Self {
        if let Err(e) = self.validate() {
            eprintln!("Errors validating configuration:");
            print_validation_errors(&e, "  ", 1);
            std::process::exit(1);
        }

        self
    }

    pub const fn build_date() -> &'static str {
        BUILD_DATE
    }

    pub const fn rustc_version() -> &'static str {
        BUILD_RUSTC_VERSION
    }

    pub const fn app_name_with_version() -> &'static str {
        APPLICATION_NAME_WITH_VERSION
    }

    pub const fn app_version() -> &'static str {
        APPLICATION_VERSION
    }
}
