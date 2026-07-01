use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use anyhow::anyhow;
use app_helpers::{
    ffprobe::{self, FfProbeResult, Stream},
    id::time_thread_id,
    temp_dir::TempDir,
    trash::move_to_trash,
};
use futures::future::BoxFuture;
use image::ColorType;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{fs, process::Command};
use tracing::{debug, error, trace};

use crate::{
    config::ActionsConfig,
    fixers::{
        Fixer, FixerReturn, IntoFixerReturn,
        common::{FixRequest, FixResult, FixerError},
    },
};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MediaFormats;

#[async_trait::async_trait]
#[typetag::serde]
impl Fixer for MediaFormats {
    fn description(&self) -> &'static str {
        "Re-encode files to match more standard formats (eg. webm -> mp4)."
    }

    /// Options:
    ///
    async fn run(&self, request: &FixRequest) -> FixerReturn {
        convert_into_preferred_formats(request.clone()).await
    }
}

async fn convert_into_preferred_formats(request: FixRequest) -> FixerReturn {
    let file_path = request.file_path.clone();
    debug!("Checking if {file_path:?} has unwanted formats");

    check_and_fix_file(&file_path)
        .await
        .map(|p| {
            debug!("File {file_path:?} done being converted");
            FixResult::new(request, p)
        })
        .map_err(FixerError::failed_fix)
}

async fn check_and_fix_file(file_path: &Path) -> Result<PathBuf, MediaFormatsError> {
    let file_format_info = ffprobe::ffprobe_async(file_path).await?;

    trace!(
        "File format info: {file_format_info:?}",
        file_format_info = file_format_info
    );

    let file_media_stream = {
        let file_image_stream = file_format_info.streams.iter().find(|s| {
            s.codec_type
                .as_deref()
                .is_some_and(|codec| matches!(codec, "video" | "image"))
        });

        file_image_stream
            .or_else(|| {
                file_format_info.streams.iter().find(|s| {
                    s.codec_type
                        .as_deref()
                        .is_some_and(|codec| codec == "audio")
                })
            })
            .ok_or_else(|| MediaFormatsError::NoMediaStream(file_path.to_path_buf()))?
    }
    .clone();

    let file_stream_codec = file_media_stream
        .codec_name
        .as_deref()
        .ok_or_else(|| MediaFormatsError::CodecName(file_path.to_path_buf()))?;

    trace!(
        "File stream codec: {file_stream_codec:?}",
        file_stream_codec = file_stream_codec
    );

    let handler = CODEC_HANDLERS
        .iter()
        .find(|h| (h.can_handle)(file_stream_codec, &file_media_stream));

    if let Some(handler) = handler {
        trace!("Using handler: {handler:?}", handler = handler);
        return (handler.handle)(file_format_info, file_media_stream)
            .await
            .map_err(MediaFormatsError::CodecFix);
    }

    error!("File {path:?} has unknown codec", path = file_path);

    Err(MediaFormatsError::UnknownCodec(
        file_stream_codec.to_string(),
    ))
}

#[derive(Debug, Clone, PartialEq, Default)]
struct TranscodeInfo {
    extension: &'static str,
    video_codec: Option<&'static str>,
    audio_codec: Option<&'static str>,
    additional_args: Vec<&'static str>,
}

impl TranscodeInfo {
    fn new(extension: &'static str) -> Self {
        Self {
            extension,
            ..Default::default()
        }
    }

    const fn with_video_codec(mut self, video_codec: &'static str) -> Self {
        self.video_codec = Some(video_codec);
        self
    }

    const fn with_audio_codec(mut self, audio_codec: &'static str) -> Self {
        self.audio_codec = Some(audio_codec);
        self
    }

    fn with_additional_args<I>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = &'static str>,
    {
        self.additional_args.extend(args);
        self
    }

    // Default codecs

    fn mp3() -> Self {
        Self::new("mp3").with_audio_codec("mp3")
    }

    fn mp4() -> Self {
        Self::new("mp4")
            .with_video_codec("libx264")
            .with_audio_codec("aac")
            .with_additional_args(["-map_metadata", "-1"])
    }

    fn jpg() -> Self {
        Self::new("jpg")
            .with_video_codec("mjpeg")
            .with_additional_args(["-map_metadata", "-1"])
    }

    fn png() -> Self {
        Self::new("png")
            .with_video_codec("png")
            .with_additional_args(["-map_metadata", "-1"])
    }
}

async fn transcode_media_into(
    from_path: &Path,
    to_format: &TranscodeInfo,
) -> anyhow::Result<PathBuf> {
    trace!("Transcoding {from_path:?} with {to_format:?}");

    let to_extension = to_format.extension;

    let cache_folder = TempDir::absolute(
        ActionsConfig::cache_dir().join(format!("transcode-{}", time_thread_id())),
    )
    .map_err(|e| anyhow!("Failed to create temporary directory: {e:?}"))?;

    let cache_from_path = copy_file_to_cache_folder(cache_folder.path(), from_path).await?;

    let cache_to_path = {
        let path = cache_from_path.with_extension(to_extension);

        if path_has_extension(&path, to_extension) {
            let new_file_name =
                file_name_with_suffix_extension(&path, "transcoded").ok_or_else(|| {
                    anyhow!(
                        "Failed to get file name with suffix extension of {path:?}",
                        path = path,
                    )
                })?;
            path.with_file_name(new_file_name)
        } else {
            path
        }
    };

    debug!(
        "Converting {from:?} to {to:?}",
        from = cache_from_path.file_name(),
        to = cache_to_path.file_name(),
    );

    let ffmpeg_path = ActionsConfig::dependency_paths().ffmpeg_path();
    trace!("`ffmpeg' binary: {ffmpeg_path:?}");
    let mut cmd = Command::new(ffmpeg_path);
    let mut cmd = cmd
        .arg("-y")
        .arg("-hide_banner")
        .args(["-loglevel", "panic"])
        .arg("-i")
        .arg(&cache_from_path)
        .args(["-max_muxing_queue_size", "1024"])
        .args(["-vf", "scale=ceil(iw/2)*2:ceil(ih/2)*2"])
        .args(["-b:a", "256k"])
        .args(["-preset", "slow"]);

    if let Some(video_codec) = to_format.video_codec {
        cmd = cmd.args(["-c:v", video_codec]);
    }

    if let Some(audio_codec) = to_format.audio_codec {
        cmd = cmd.args(["-c:a", audio_codec]);
    }

    cmd = cmd.args(&to_format.additional_args);

    cmd = cmd.stdin(Stdio::null());

    let cmd = cmd.arg(&cache_to_path);
    debug!("Running `ffmpeg' command: {cmd:?}");

    let cmd_output = cmd.output().await;
    match cmd_output {
        Ok(std::process::Output { status, .. }) if status.success() && cache_to_path.exists() => {
            debug!(
                "Converted file {from:?} to {to}",
                from = from_path,
                to = to_extension
            );

            let new_file_path = from_path.with_extension(to_extension);

            trace!(
                "Copying {cache_path:?} to {new_path:?}",
                cache_path = cache_to_path,
                new_path = new_file_path
            );
            if let Err(e) = fs::copy(&cache_to_path, &new_file_path).await {
                return Err(anyhow!(
                    "Failed to copy {from:?} to {to:?}: {e:?}",
                    from = cache_to_path,
                    to = new_file_path,
                ));
            }

            if new_file_path != from_path {
                trace!("Deleting old file {path:?}", path = from_path);
                if let Err(e) = move_to_trash(from_path) {
                    debug!("Failed to delete {path:?}: {e:?}", path = from_path);
                }
            }

            Ok(new_file_path)
        }
        _ => Err(anyhow!(
            "Failed transforming {from_path:?} into {to_extension}"
        )),
    }
}

async fn copy_file_to_cache_folder(
    cache_folder: &Path,
    file_path: &Path,
) -> anyhow::Result<PathBuf> {
    trace!("Using {path:?} as cache folder", path = cache_folder);

    let cache_file_path = {
        let filename = file_path
            .file_name()
            .ok_or_else(|| anyhow!("Failed to get file name of {path:?}", path = file_path))?;

        cache_folder.join(filename)
    };

    trace!(
        "Copying {from:?} to {to:?}",
        from = file_path,
        to = cache_file_path,
    );
    fs::copy(file_path, &cache_file_path).await.map_err(|e| {
        anyhow!(
            "Failed to copy {from:?} to {to:?}: {e:?}",
            from = file_path,
            to = cache_file_path,
        )
    })?;

    Ok(cache_file_path)
}

fn path_has_extension(path: &Path, wanted_extension: &str) -> bool {
    path.extension().and_then(std::ffi::OsStr::to_str) == Some(wanted_extension)
}

fn file_name_with_suffix_extension(path: &Path, suffix: &str) -> Option<PathBuf> {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .map(|ext| format!("{suffix}.{ext}", ext = ext, suffix = suffix))
        .map(|ext| path.with_extension(ext))
        .and_then(|p| p.file_name().map(PathBuf::from))
}

fn get_stream_of_type<'a>(
    file_format_info: &'a FfProbeResult,
    stream_type: &'a str,
) -> Option<&'a Stream> {
    file_format_info
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref().is_some_and(|x| x == stream_type))
}

#[derive(Debug, Clone)]
struct CodecHandler {
    pub can_handle: fn(&str, &Stream) -> bool,
    pub handle: fn(FfProbeResult, Stream) -> BoxFuture<'static, anyhow::Result<PathBuf>>,
}

const CODEC_HANDLERS: &[CodecHandler] = &[
    CodecHandler {
        can_handle: |codec, _stream| matches!(codec, "mp3"),
        handle: |file_format_info, _matched_stream| {
            Box::pin(async move {
                let from_path = PathBuf::from(file_format_info.format.filename.clone());

                trace!(
                    "File {path:?} is already in preferred format",
                    path = from_path
                );

                Ok(from_path)
            })
        },
    },
    CodecHandler {
        can_handle: |codec, stream| {
            matches!(stream.codec_type.as_deref(), Some("audio")) && !matches!(codec, "mp3")
        },
        handle: |file_format_info, _matched_stream| {
            Box::pin(async move {
                let file_path = PathBuf::from(file_format_info.format.filename.clone());
                transcode_media_into(&file_path, &TranscodeInfo::mp3()).await
            })
        },
    },
    CodecHandler {
        can_handle: |codec, _stream| matches!(codec, "h264"),
        handle: |file_format_info, video_stream| {
            Box::pin(async move {
                let file_path = PathBuf::from(file_format_info.format.filename.clone());

                let video_codec_ok = video_stream
                    .codec_name
                    .as_ref()
                    .is_some_and(|vcodec| vcodec == "h264");

                let audio_codec_ok =
                    get_stream_of_type(&file_format_info, "audio").is_none_or(|audio_stream| {
                        audio_stream
                            .codec_name
                            .as_ref()
                            .is_some_and(|acodec| acodec == "aac")
                    });

                let extension_ok = path_has_extension(&file_path, "mp4");

                trace!(
                    "Video codec ok: {video_codec_ok:?} | Audio codec ok: {audio_codec_ok:?} | \
                     Extension ok: {extension_ok:?}",
                    video_codec_ok = video_codec_ok,
                    audio_codec_ok = audio_codec_ok,
                    extension_ok = extension_ok,
                );

                if video_codec_ok && audio_codec_ok && extension_ok {
                    trace!(
                        "File {path:?} is already in preferred format",
                        path = file_path
                    );

                    return Ok(file_path);
                }

                transcode_media_into(&file_path, &TranscodeInfo::mp4()).await
            })
        },
    },
    CodecHandler {
        can_handle: |codec, _stream| matches!(codec, "mpeg4" | "vp8" | "vp9" | "av1" | "hevc"),
        handle: |file_format_info, _matched_stream| {
            Box::pin(async move {
                let from_path = PathBuf::from(file_format_info.format.filename.clone());
                trace!("Converting {path:?} into mp4", path = from_path);
                transcode_media_into(&from_path, &TranscodeInfo::mp4()).await
            })
        },
    },
    CodecHandler {
        can_handle: |codec, _stream| matches!(codec, "png" | "mjpeg" | "gif"),
        handle: |file_format_info, _matched_stream| {
            Box::pin(async move {
                let from_path = PathBuf::from(file_format_info.format.filename.clone());

                trace!(
                    "File {path:?} is already in preferred format",
                    path = from_path
                );

                Ok(from_path)
            })
        },
    },
    CodecHandler {
        can_handle: |codec, _stream| matches!(codec, "webp"),
        handle: |file_format_info, _matched_stream| {
            Box::pin(async move {
                let from_path = PathBuf::from(file_format_info.format.filename.clone());
                let img = image::open(&from_path)?;
                let color = img.color();

                match color {
                    ColorType::Rgb8 | ColorType::Rgb16 | ColorType::Rgb32F => {
                        trace!("Converting {path:?} into jpg", path = from_path);
                        transcode_media_into(&from_path, &TranscodeInfo::jpg()).await
                    }
                    ColorType::Rgba8 | ColorType::Rgba16 | ColorType::Rgba32F => {
                        trace!("Converting {path:?} into png", path = from_path);
                        transcode_media_into(&from_path, &TranscodeInfo::png()).await
                    }

                    color_type => {
                        error!(
                            "File {path:?} has unknown color type {color_type:?}",
                            path = from_path,
                            color_type = color_type,
                        );

                        Err(anyhow!(
                            "File has an unknown color type ({color_type:?}), please report this \
                             issue to the developers.",
                            color_type = color_type,
                        ))
                    }
                }
            })
        },
    },
];

#[derive(Debug, Error)]
pub enum MediaFormatsError {
    #[error(transparent)]
    FfProbeError(#[from] ffprobe::FfProbeError),
    #[error("Failed to get media stream of {0:?}")]
    NoMediaStream(PathBuf),
    #[error("Failed to get codec of {0:?}")]
    CodecName(PathBuf),
    #[error(transparent)]
    CodecFix(anyhow::Error),
    #[error("File has an unknown codec ({0:?}), please report this issue to the developers.")]
    UnknownCodec(String),
}

impl IntoFixerReturn for MediaFormatsError {
    fn into_fixer_return(self) -> FixerReturn {
        Err(FixerError::failed_fix(self))
    }
}
