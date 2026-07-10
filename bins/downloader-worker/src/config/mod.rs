use std::path::PathBuf;

use app_config::{
    BUILD_DATE, BUILD_RUSTC_VERSION, Dumpable, GlobalConfig, LogFormat, common::APPLICATION_NAME,
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
    /// If set, the log will be written to this file as well as stdout
    #[clap(long, env = "DOWNLOADER_HUB_WORKER_LOG_FILE")]
    pub log_file: Option<PathBuf>,

    /// Log format for stderr
    #[clap(long, env = "DOWNLOADER_HUB_LOG_FORMAT", default_value = "pretty")]
    pub log_format: LogFormat,

    /// Log format for the log file (when `--log-file` is set)
    #[clap(long, env = "DOWNLOADER_HUB_LOG_FILE_FORMAT", default_value = "plain")]
    pub log_file_format: LogFormat,

    /// Log level for the log file (when `--log-file` is set).
    /// If not set, falls back to `DOWNLOADER_HUB_LOG_FILE_LEVEL` or `DOWNLOADER_HUB_LOG_LEVEL`.
    #[clap(long)]
    pub log_file_level: Option<String>,

    #[clap(subcommand)]
    #[validate(nested)]
    pub cmd: CmdConfig,

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
        self.cmd = self.cmd.resolve_paths();

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
