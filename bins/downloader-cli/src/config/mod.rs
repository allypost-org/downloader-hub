use std::path::PathBuf;

use app_config::{
    Dumpable, GlobalConfig,
    common::{self},
    validators::{
        directory::{validate_is_writable_directory, value_parser_parse_valid_directory},
        file::{validate_is_files, value_parser_parse_valid_file},
        print_validation_errors,
    },
};
use clap::{Args, Parser, ValueHint};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(
    Debug, Default, Clone, Serialize, Deserialize, Parser, Validate, GlobalConfig, Dumpable,
)]
pub struct Config {
    #[clap(flatten)]
    #[validate(nested)]
    #[serde(skip)]
    pub run: RunConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub endpoint: common::EndpointConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub dependency_paths: common::ProgramPathConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub disabled_entries: common::DisabledEntriesConfig,

    #[clap(flatten)]
    #[validate(nested)]
    pub request: common::RequestConfig,

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

        {
            let parsed = parsed.clone();
            app_actions::config::init(
                parsed.endpoint,
                parsed.dependency_paths,
                parsed.disabled_entries.entries,
                parsed.request,
            )?;
        }

        Self::init(parsed)
    }

    pub fn run() -> &'static RunConfig {
        &Self::global().run
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
}

#[derive(Debug, Clone, Default, Args, Serialize, Deserialize, Validate)]
pub struct RunConfig {
    #[clap(flatten)]
    #[validate(nested)]
    #[serde(skip)]
    pub entries_group: UrlGroup,

    /// Directory to download files to
    ///
    /// Will be created if it doesn't exist.
    ///
    /// Will error if it is not a valid path.
    #[clap(short = 'd', long, default_value = ".", value_hint = ValueHint::FilePath, value_parser = value_parser_parse_valid_directory())]
    #[validate(custom(function = "validate_is_writable_directory"))]
    pub output_directory: PathBuf,

    /// Rename file paths passed to the command to the standard format.
    ///
    /// Newly downloaded files are unaffected as they are
    /// already named correctly.
    ///
    /// The standard format is `<id>.<original_name>.<extension>`.
    #[clap(long, action = clap::ArgAction::SetTrue)]
    pub and_rename: bool,
}

#[derive(Debug, Clone, Default, Args, Serialize, Deserialize, Validate)]
#[group(required = true, multiple = true)]
pub struct UrlGroup {
    /// URLs to download.
    ///
    /// Has the same behaviour as specifying the entry as a raw argument.
    /// Will be checked whether they are valid urls or not.
    ///
    /// Errors will be thrown if any urls are invalid.
    #[clap(short = 'u', long = "url")]
    pub urls: Vec<String>,

    /// Paths to fix.
    ///
    /// Paths will be resolved and checked whether they are valid paths or not.
    ///
    /// Errors will be thrown if any paths are invalid or if they don't exist.
    #[clap(short = 'f', long = "file", value_hint = ValueHint::FilePath, value_parser = value_parser_parse_valid_file())]
    #[validate(custom(function = "validate_is_files"))]
    pub files: Vec<PathBuf>,

    /// Paths to split and fix.
    ///
    /// Paths will be resolved and checked whether they are valid paths or not.
    ///
    /// Errors will be thrown if any paths are invalid or if they don't exist.
    #[clap(short = 's', long = "split-file", value_hint = ValueHint::FilePath, value_parser = value_parser_parse_valid_file())]
    #[validate(custom(function = "validate_is_files"))]
    pub split_files: Vec<PathBuf>,

    /// Download entry to process
    ///
    /// Entry can be either an url or a path.
    /// Multiple entries can be specified.
    ///
    /// If a path is specified, the file at the path will be run through fixers, and urls will be downloaded.
    ///
    /// Invalid entries will be _ignored_.
    #[clap(id = "URL_OR_FILE", value_hint = ValueHint::FilePath)]
    pub urls_or_files: Vec<DownloadEntry>,
}

pub type DownloadEntry = String;
