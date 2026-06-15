use std::path::{Path, PathBuf};

use clap::{Args, ValueHint};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::validators::file::{validate_is_file, value_parser_parse_valid_file};

#[derive(Debug, Clone, Default, Serialize, Deserialize, Args, Validate)]
#[allow(clippy::struct_field_names)]
#[clap(next_help_heading = Some("Program paths"))]
pub struct ProgramPathConfig {
    /// Path to the yt-dlp executable.
    ///
    /// If not provided, yt-dlp will be searched for in $PATH
    #[arg(long, default_value = None, env = "DOWNLOADER_HUB_YT_DLP", value_hint = ValueHint::FilePath, value_parser = value_parser_parse_valid_file())]
    #[validate(custom(function = "validate_is_file"), required)]
    yt_dlp_path: Option<PathBuf>,

    /// Path to the ffmpeg executable.
    ///
    /// If not provided, ffmpeg will be searched for in $PATH
    #[arg(long, default_value = None, env = "DOWNLOADER_HUB_FFMPEG", value_hint = ValueHint::FilePath, value_parser = value_parser_parse_valid_file())]
    #[validate(custom(function = "validate_is_file"), required)]
    ffmpeg_path: Option<PathBuf>,

    /// Path to the ffprobe executable.
    ///
    /// If not provided, ffprobe will be searched for in $PATH
    #[arg(long, default_value = None, env = "DOWNLOADER_HUB_FFPROBE", value_hint = ValueHint::FilePath, value_parser = value_parser_parse_valid_file())]
    #[validate(custom(function = "validate_is_file"), required)]
    ffprobe_path: Option<PathBuf>,

    /// Path to the scenedetect executable.
    ///
    /// If not provided, scenedetect will be searched for in $PATH
    #[arg(long, default_value = None, env = "DOWNLOADER_HUB_SCENEDETECT", value_hint = ValueHint::FilePath, value_parser = value_parser_parse_valid_file())]
    #[validate(custom(function = "validate_is_file"))]
    scenedetect_path: Option<PathBuf>,

    /// Path to the imagemagick executable.
    ///
    /// If not provided, imagemagick will be searched for in $PATH
    #[arg(long, default_value = None, env = "DOWNLOADER_HUB_IMAGEMAGICK", value_hint = ValueHint::FilePath, value_parser = value_parser_parse_valid_file())]
    #[validate(custom(function = "validate_is_file"))]
    imagemagick_path: Option<PathBuf>,
}
impl ProgramPathConfig {
    #[must_use]
    pub fn yt_dlp_path(&self) -> &Path {
        self.yt_dlp_path.as_ref().expect(
            "`yt-dlp` executable not found. Please make sure it is installed and added to the \
             PATH environment variable.",
        )
    }

    #[must_use]
    pub fn ffmpeg_path(&self) -> &Path {
        self.ffmpeg_path.as_ref().expect(
            "`ffmpeg` executable not found. Please make sure it is installed and added to the \
             PATH environment variable.",
        )
    }

    #[must_use]
    pub fn ffprobe_path(&self) -> &Path {
        self.ffprobe_path.as_ref().expect(
            "`ffprobe` executable not found. Please make sure it is installed and added to the \
             PATH environment variable.",
        )
    }

    #[must_use]
    pub fn scenedetect_path(&self) -> Option<PathBuf> {
        self.scenedetect_path.clone()
    }

    #[must_use]
    pub fn imagemagick_path(&self) -> Option<PathBuf> {
        self.imagemagick_path.clone()
    }

    #[must_use]
    pub fn resolve_paths(mut self) -> Self {
        self.with_resolved_paths();
        self
    }

    pub fn with_resolved_paths(&mut self) -> &Self {
        self.yt_dlp_path = self
            .yt_dlp_path
            .clone()
            .or_else(|| which::which("yt-dlp").ok());
        self.ffmpeg_path = self
            .ffmpeg_path
            .clone()
            .or_else(|| which::which("ffmpeg").ok());
        self.ffprobe_path = self
            .ffprobe_path
            .clone()
            .or_else(|| which::which("ffprobe").ok());
        self.scenedetect_path = self
            .scenedetect_path
            .clone()
            .or_else(|| which::which("scenedetect").ok());
        self.imagemagick_path = self
            .imagemagick_path
            .clone()
            .or_else(|| which::which("magick").ok());
        self
    }
}
