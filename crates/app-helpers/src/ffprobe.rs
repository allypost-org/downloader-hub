use std::{
    collections::HashMap,
    io, num,
    path::Path,
    process::{self, Stdio},
    time,
};

use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::config::HelpersConfig;

pub fn ffprobe(path: impl AsRef<Path>) -> Result<FfProbeResult, FfProbeError> {
    ffprobe_config(
        FfprobeConfig {
            count_frames: false,
        },
        path,
    )
}
pub async fn ffprobe_async<T>(path: T) -> Result<FfProbeResult, FfProbeError>
where
    T: AsRef<Path> + Send,
{
    ffprobe_config_async(
        FfprobeConfig {
            count_frames: false,
        },
        path,
    )
    .await
}

pub async fn ffprobe_config_async<T>(
    config: FfprobeConfig,
    path: T,
) -> Result<FfProbeResult, FfProbeError>
where
    T: AsRef<Path> + Send,
{
    let path = path.as_ref();

    let ffprobe_path = HelpersConfig::dependency_paths().ffprobe_path();
    let mut cmd = Command::new(ffprobe_path);
    {
        cmd.args(["-v", "quiet"])
            .args(["-print_format", "json=c=1"])
            .arg("-show_format")
            .arg("-show_streams");

        if config.count_frames {
            cmd.arg("-count_frames");
        }

        cmd.arg(path);
        cmd.stdin(Stdio::null());
    }

    let out = cmd.output().await.map_err(FfProbeError::Io)?;

    if !out.status.success() {
        return Err(FfProbeError::Status(out));
    }

    serde_json::from_slice::<FfProbeResult>(&out.stdout).map_err(FfProbeError::Deserialize)
}

pub fn ffprobe_config(
    config: FfprobeConfig,
    path: impl AsRef<Path>,
) -> Result<FfProbeResult, FfProbeError> {
    let path = path.as_ref();

    let ffprobe_path = HelpersConfig::dependency_paths().ffprobe_path();
    let mut cmd = process::Command::new(ffprobe_path);
    {
        cmd.args(["-v", "quiet"])
            .args(["-print_format", "json=c=1"])
            .arg("-show_format")
            .arg("-show_streams");

        if config.count_frames {
            cmd.arg("-count_frames");
        }

        cmd.arg(path);
        cmd.stdin(Stdio::null());
    }

    let out = cmd.output().map_err(FfProbeError::Io)?;

    if !out.status.success() {
        return Err(FfProbeError::Status(out));
    }

    serde_json::from_slice::<FfProbeResult>(&out.stdout).map_err(FfProbeError::Deserialize)
}

/// ffprobe configuration.
///
/// Use [`Config::builder`] for constructing a new config.
#[derive(Clone, Copy, Debug)]
pub struct FfprobeConfig {
    count_frames: bool,
}

impl FfprobeConfig {
    /// Construct a new `ConfigBuilder`.
    #[must_use]
    pub const fn builder() -> ConfigBuilder {
        ConfigBuilder::new()
    }
}

/// Build the ffprobe configuration.
pub struct ConfigBuilder {
    config: FfprobeConfig,
}

impl ConfigBuilder {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            config: FfprobeConfig {
                count_frames: false,
            },
        }
    }

    /// Enable the -`count_frames` setting.
    /// Will fully decode the file and count the frames.
    /// Frame count will be available in [`Stream::nb_read_frames`].
    #[must_use]
    pub const fn count_frames(mut self, count_frames: bool) -> Self {
        self.config.count_frames = count_frames;
        self
    }

    /// Finalize the builder into a [`Config`].
    #[must_use]
    pub const fn build(self) -> FfprobeConfig {
        self.config
    }

    /// Run ffprobe with the config produced by this builder.
    pub fn run(self, path: impl AsRef<Path>) -> Result<FfProbeResult, FfProbeError> {
        ffprobe_config(self.config, path)
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FfProbeError {
    #[error(transparent)]
    Io(io::Error),
    #[error("ffprobe exited with status code {}: {}", .0.status, String::from_utf8_lossy(&.0.stderr).trim())]
    Status(process::Output),
    #[error(transparent)]
    Deserialize(serde_json::Error),
    #[error("Missing binary: {0}")]
    MissingBinary(String),
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FfProbeResult {
    pub streams: Vec<Stream>,
    pub format: Format,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stream {
    pub index: i64,
    pub codec_name: Option<String>,
    pub sample_aspect_ratio: Option<String>,
    pub display_aspect_ratio: Option<String>,
    pub color_range: Option<String>,
    pub color_space: Option<String>,
    pub bits_per_raw_sample: Option<String>,
    pub channel_layout: Option<String>,
    pub max_bit_rate: Option<String>,
    pub nb_frames: Option<String>,
    /// Number of frames seen by the decoder.
    /// Requires full decoding and is only available if the `count_frames`
    /// setting was enabled.
    pub nb_read_frames: Option<String>,
    pub codec_long_name: Option<String>,
    pub codec_type: Option<String>,
    pub codec_time_base: Option<String>,
    pub codec_tag_string: String,
    pub codec_tag: String,
    pub sample_fmt: Option<String>,
    pub sample_rate: Option<String>,
    pub channels: Option<i64>,
    pub bits_per_sample: Option<i64>,
    pub r_frame_rate: String,
    pub avg_frame_rate: String,
    pub time_base: String,
    pub start_pts: Option<i64>,
    pub start_time: Option<String>,
    pub duration_ts: Option<i64>,
    pub duration: Option<String>,
    pub bit_rate: Option<String>,
    pub disposition: Disposition,
    pub tags: Option<StreamTags>,
    pub profile: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub coded_width: Option<i64>,
    pub coded_height: Option<i64>,
    pub closed_captions: Option<i64>,
    pub has_b_frames: Option<i64>,
    pub pix_fmt: Option<String>,
    pub level: Option<i64>,
    pub chroma_location: Option<String>,
    pub refs: Option<i64>,
    pub is_avc: Option<String>,
    pub nal_length: Option<String>,
    pub nal_length_size: Option<String>,
    pub field_order: Option<String>,
    pub id: Option<String>,
    #[serde(default)]
    pub side_data_list: Vec<SideData>,
}
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
// Allowed to prevent having to break compatibility of float fields are added.
#[allow(clippy::derive_partial_eq_without_eq)]
pub struct SideData {
    pub side_data_type: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
// Allowed to prevent having to break compatibility of float fields are added.
#[allow(clippy::derive_partial_eq_without_eq)]
pub struct Disposition {
    pub default: i64,
    pub dub: i64,
    pub original: i64,
    pub comment: i64,
    pub lyrics: i64,
    pub karaoke: i64,
    pub forced: i64,
    pub hearing_impaired: i64,
    pub visual_impaired: i64,
    pub clean_effects: i64,
    pub attached_pic: i64,
    pub timed_thumbnails: i64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
// Allowed to prevent having to break compatibility of float fields are added.
#[allow(clippy::derive_partial_eq_without_eq)]
pub struct StreamTags {
    pub language: Option<String>,
    pub creation_time: Option<String>,
    pub handler_name: Option<String>,
    pub encoder: Option<String>,
    pub timecode: Option<String>,
    pub reel_name: Option<String>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Format {
    pub filename: String,
    pub nb_streams: i64,
    pub nb_programs: i64,
    pub format_name: String,
    pub format_long_name: String,
    pub start_time: Option<String>,
    pub duration: Option<String>,
    pub size: Option<String>,
    pub bit_rate: Option<String>,
    pub probe_score: i64,
    pub tags: Option<FormatTags>,
}

impl Format {
    /// Get the duration parsed into a [`std::time::Duration`].
    #[must_use]
    pub fn try_get_duration(&self) -> Option<Result<time::Duration, num::ParseFloatError>> {
        self.duration
            .as_ref()
            .map(|duration| match duration.parse::<f64>() {
                Ok(num) => Ok(time::Duration::from_secs_f64(num)),
                Err(error) => Err(error),
            })
    }

    /// Get the duration parsed into a [`std::time::Duration`].
    ///
    /// Will return [`None`] if no duration is available, or if parsing fails.
    /// See [`Self::try_get_duration`] for a method that returns an error.
    #[must_use]
    pub fn get_duration(&self) -> Option<time::Duration> {
        self.try_get_duration()?.ok()
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
pub struct FormatTags {
    #[serde(rename = "WMFSDKNeeded")]
    pub wmf_sdk_needed: Option<String>,
    #[serde(rename = "WMFSDKVersion")]
    pub wmf_sdk_version: Option<String>,
    #[serde(rename = "DeviceConformanceTemplate")]
    pub device_conformance_template: Option<String>,
    #[serde(rename = "IsVBR")]
    pub is_vbr: Option<String>,
    pub major_brand: Option<String>,
    pub minor_version: Option<String>,
    pub compatible_brands: Option<String>,
    pub creation_time: Option<String>,
    pub encoder: Option<String>,

    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
