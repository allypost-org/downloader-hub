use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Stdio,
};

use app_helpers::{ffprobe, temp_dir::TempDir, trash::move_to_trash};
use serde::{Deserialize, Serialize};
use tokio::{fs, process::Command};
use tracing::{debug, trace, warn};

use crate::{
    config::ActionsConfig,
    fixers::{
        Fixer, FixerReturn, IntoFixerReturn,
        common::{
            FixRequest, FixResult,
            command::CmdError,
            crop_filter::{CropError, CropFilter},
        },
    },
};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CropVideoBars;

#[async_trait::async_trait]
#[typetag::serde]
impl Fixer for CropVideoBars {
    fn can_run(&self) -> bool {
        ActionsConfig::dependency_paths()
            .imagemagick_path()
            .is_some()
    }

    async fn can_run_for(&self, request: &FixRequest) -> bool {
        let Ok(media_info) = ffprobe::ffprobe_async(&request.file_path).await else {
            return false;
        };

        if media_info.format.format_name == "image2" {
            return false;
        }

        media_info
            .streams
            .iter()
            .any(|s| s.codec_type.as_ref().is_some_and(|x| x == "video"))
    }

    fn description(&self) -> &'static str {
        "Crops dead space around the video. Supports both black and white outlines."
    }

    /// Options:
    ///
    async fn run(&self, request: &FixRequest) -> FixerReturn {
        do_auto_crop_video(&request.file_path)
            .await
            .map(|x| FixResult::new(request.clone(), x))
            .into_fixer_return()
    }
}

async fn do_auto_crop_video(file_path: &Path) -> Result<PathBuf, CropError> {
    debug!("Auto cropping video {file_path:?}");

    let video_stream = get_video_stream(file_path).await?;

    let (w, h) = {
        let video_stream = if let Some(s) = video_stream {
            trace!("Found video stream");
            s
        } else {
            debug!("File does not contain a video stream, skipping");
            return Ok(file_path.into());
        };

        if let (Some(w), Some(h)) = (video_stream.width, video_stream.height) {
            trace!("Video width: {w}, height: {h}");
            (w, h)
        } else {
            return Err(CropError::NoDimensions(file_path.to_path_buf()));
        }
    };

    let final_crop_filter = match generate_crop_filter(file_path).await {
        Ok(mut f) => {
            f.intersect(&CropFilter {
                width: w,
                height: h,
                x: 0,
                y: 0,
            });
            f
        }
        Err(e) => {
            debug!("Failed to generate crop filter: {e:?}");
            return Err(e);
        }
    };

    debug!("Final crop filter: {final_crop_filter:?}");

    if final_crop_filter.width >= w && final_crop_filter.height >= h {
        debug!("Video is already cropped, skipping");
        return Ok(file_path.into());
    }

    let new_filename = {
        let file_name = file_path
            .file_stem()
            .and_then(OsStr::to_str)
            .ok_or_else(|| CropError::NoFilePart("stem".to_string(), file_path.to_path_buf()))?;

        let file_extension = file_path
            .extension()
            .and_then(OsStr::to_str)
            .ok_or_else(|| {
                CropError::NoFilePart("extension".to_string(), file_path.to_path_buf())
            })?;

        file_path.with_file_name(format!("{file_name}.ac.{file_extension}"))
    };

    trace!(?new_filename, "Using new filename for file");

    let mut cmd = Command::new(ActionsConfig::dependency_paths().ffmpeg_path());
    let res = cmd
        .arg("-y")
        .args(["-loglevel", "panic"])
        .arg("-i")
        .arg(file_path)
        .args(["-vf", &final_crop_filter.to_string()])
        .args(["-map_metadata", "0", "-movflags", "use_metadata_tags"])
        .args(["-preset", "slow"])
        .arg(&new_filename)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .status()
        .await
        .map_err(|e| CropError::CommandError(CmdError::Run(e)))?;

    if !res.success() {
        return Err(CropError::CommandError(CmdError::FailedStatus(
            "Failed to extract crop data from video frames".into(),
            res,
        )));
    }

    if let Err(e) = move_to_trash(file_path) {
        warn!("Failed to move file {file_path:?} to trash: {e:?}");
    }

    Ok(new_filename)
}

async fn generate_crop_filter(file_path: &Path) -> Result<CropFilter, CropError> {
    debug!(?file_path, "Generating crop filter");

    let tmp_dir = match TempDir::in_tmp_with_prefix("downloader-hub.crop-video-bars.") {
        Ok(d) => d,
        Err(e) => return Err(CropError::TempDirError(e)),
    };
    trace!(?tmp_dir, "Created temp dir to write frames to");

    let mut cmd = Command::new(ActionsConfig::dependency_paths().ffmpeg_path());
    let res = cmd
        .arg("-y")
        .arg("-i")
        .arg(file_path)
        .args(["-vf", "fps=1"])
        .arg(format!("{}/%0d.jpg", tmp_dir.path().to_string_lossy()))
        .kill_on_drop(true);
    debug!(cmd = ?res.as_std(), "Running command to split video into frames");
    let res = res
        .output()
        .await
        .map_err(|e| CropError::CommandError(CmdError::Run(e)))?;

    if !res.status.success() {
        return Err(CropError::CommandError(CmdError::Failed(
            "Failed to convert video to frames".into(),
            res.into(),
        )));
    }

    debug!("Generated frames successfully");

    let files_in_tmp_dir = {
        let tmp_path = tmp_dir.path();
        debug!(?tmp_path, "Listing frame files in temp dir");
        let mut dir_iter = fs::read_dir(tmp_path)
            .await
            .map_err(CropError::TempDirError)?;

        let mut entries = Vec::new();
        while let Ok(Some(entry)) = dir_iter.next_entry().await {
            let path_in_temp_dir = entry.path();

            let file_name = match path_in_temp_dir.file_name() {
                Some(x) => x,
                None => continue,
            };

            let path_in_dir = tmp_path.join(file_name);

            entries.push(path_in_dir);
        }
        trace!(entries = ?entries.len(), "Found all frame files in directory");
        entries
    };

    let filter = CropFilter::from_image_files(&files_in_tmp_dir).await?;
    debug!(?filter, "Got crop filter");

    Ok(filter)
}

async fn get_video_stream(file_path: &Path) -> Result<Option<ffprobe::Stream>, CropError> {
    let media_info = ffprobe::ffprobe_async(file_path)
        .await
        .map_err(CropError::FfProbeError)?;

    let video_stream = media_info
        .streams
        .iter()
        .find(|s| s.codec_type.as_ref().is_some_and(|x| x == "video"));

    trace!(?video_stream, "Video stream");

    Ok(video_stream.cloned())
}
