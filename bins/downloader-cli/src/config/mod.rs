use std::path::PathBuf;

use app_actions::fixers::Fixer;
use app_config::{
    BootConfig, Dumpable,
    common::{self},
    validators::{
        directory::{validate_is_writable_directory, value_parser_parse_valid_directory},
        file::{validate_is_files, value_parser_parse_valid_file},
    },
};
use clap::{Args, Parser, ValueHint};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Default, Clone, Serialize, Deserialize, Parser, Validate, Dumpable)]
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

impl BootConfig for Config {
    fn resolve_paths(mut self) -> Self {
        self.dependency_paths = self.dependency_paths.resolve_paths();
        self
    }

    fn init_parsed() -> Result<Self, String> {
        let mut parsed = Self::parse_validated();

        if parsed.run.no_auto_crop {
            parsed.disabled_entries.entries.extend([
                app_actions::fixers::handlers::crop_image::CropImage.entry_id(),
                app_actions::fixers::handlers::crop_video_bars::CropVideoBars.entry_id(),
            ]);
        }

        Ok(parsed.dump_if_needed())
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

    /// Disable automatic cropping for all files processed in this run.
    #[clap(long, action = clap::ArgAction::SetTrue)]
    pub no_auto_crop: bool,

    /// Cookie header value sent with every extraction/download request.
    ///
    /// Useful for posts that require authentication, e.g. age-gated Instagram
    /// posts: pass a logged-in `sessionid=...; ds_user_id=...` cookie.
    ///
    /// Applied to all URLs in this run.
    #[clap(long, value_name = "COOKIE")]
    pub cookie: Option<String>,
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
