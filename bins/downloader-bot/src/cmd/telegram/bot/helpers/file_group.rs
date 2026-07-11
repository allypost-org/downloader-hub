use std::{
    collections::HashMap,
    fs::Metadata,
    path::{Path, PathBuf},
    sync::Arc,
};

use app_helpers::file_type::{infer_file_type, mime};
use futures::{StreamExt, stream::FuturesUnordered};
use teloxide::types::{
    InputFile, InputMedia, InputMediaAudio, InputMediaDocument, InputMediaPhoto, InputMediaVideo,
};
use tokio::sync::Mutex;
use tracing::trace;

use crate::cmd::telegram::bot::TelegramBot;

type MediaGroup = (Vec<InputMedia>, Vec<PathBuf>);

#[tracing::instrument(skip_all)]
pub async fn files_to_input_media_groups<TFiles, TFile>(
    files: TFiles,
    max_size: u64,
) -> (Vec<MediaGroup>, Vec<(PathBuf, String)>)
where
    TFiles: IntoIterator<Item = TFile> + Send + std::fmt::Debug,
    TFile: AsRef<Path>,
{
    trace!(?files, "Getting file infos");
    let (file_info, mut failed) = infos_from_files(files).await;
    trace!(?file_info, "Got file infos");

    trace!("Converting to media files");
    let media_files = file_info.into_iter().map(|file_info| {
        let input_file = InputFile::file(file_info.path.clone());

        // Handle the GIFs as animations because Telegram
        // Also handle PNGs as documents to prevent Telegram from converting them to jpgs
        // Optional todo: Also handle silent videos as animations
        if file_info
            .mime
            .as_ref()
            .is_some_and(|x| matches!(x.essence_str(), "image/gif" | "image/png"))
        {
            trace!(?file_info, "Handling GIF or PNG as document");
            return FileInfoWithMedia {
                file_info,
                media: InputMedia::Document(InputMediaDocument::new(input_file)),
            };
        }

        let file_type = file_info
            .mime
            .as_ref()
            .map(|f| f.type_().as_str().to_lowercase());
        let media = match file_type.as_deref() {
            Some("audio") => InputMedia::Audio(InputMediaAudio::new(input_file)),
            Some("video") => InputMedia::Video(InputMediaVideo::new(input_file)),
            Some("image") if file_info.dimensions_within_limits() => {
                InputMedia::Photo(InputMediaPhoto::new(input_file))
            }
            _ => InputMedia::Document(InputMediaDocument::new(input_file)),
        };

        FileInfoWithMedia { file_info, media }
    });
    trace!(?media_files, "Converted to media files");

    let chunkable_groups = {
        #[derive(Debug, Eq, PartialEq, Hash)]
        enum ChunkGroup {
            Document,
            Audio,
            Other,
        }

        let mut groups: HashMap<ChunkGroup, Vec<FileInfoWithMedia>> = HashMap::new();
        for f in media_files {
            let group_name = match f.media {
                InputMedia::Audio(_) => ChunkGroup::Audio,
                InputMedia::Document(_) => ChunkGroup::Document,
                _ => ChunkGroup::Other,
            };

            if let Some(group) = groups.get_mut(&group_name) {
                group.push(f);
            } else {
                groups.insert(group_name, vec![f]);
            }
        }

        groups.into_values().collect::<Vec<_>>()
    };
    trace!(?chunkable_groups, "Partitioned files");

    let mut res = vec![];
    for group in chunkable_groups {
        let (chunks, failed_inner) = chunk(group, max_size);
        failed.extend(failed_inner);
        res.extend(chunks.into_iter().map(|chunk| {
            let paths = chunk
                .iter()
                .map(|item| item.file_info.path.clone())
                .collect();
            let media = chunk.into_iter().map(|item| item.media).collect();
            (media, paths)
        }));
    }
    trace!(?res, "Got file groupings");

    trace!(?failed, "Got final failed paths");

    (res, failed)
}

#[derive(Debug)]
struct FileInfo {
    path: PathBuf,
    metadata: Metadata,
    mime: Option<mime::Mime>,
    dimensions: Option<FileDimensions>,
}

impl FileInfo {
    pub const fn dimensions_within_limits(&self) -> bool {
        let Some(dimensions) = &self.dimensions else {
            return true;
        };

        dimensions.is_within_limits()
    }
}

#[derive(Debug)]
struct FileDimensions {
    width: i64,
    height: i64,
}

impl FileDimensions {
    pub const fn is_within_limits(&self) -> bool {
        self.width < 10_000
            && self.height < 10_000
            && self.width > 0
            && self.height > 0
            && self.width / self.height < 20
            && self.height / self.width < 20
    }
}

#[derive(Debug)]
struct FileInfoWithMedia {
    file_info: FileInfo,
    media: InputMedia,
}

fn chunk(
    items: Vec<FileInfoWithMedia>,
    max_size_bytes: u64,
) -> (Vec<Vec<FileInfoWithMedia>>, Vec<(PathBuf, String)>) {
    let mut failed = vec![];
    let mut res = vec![];
    let mut res_size = 0_u64;
    let mut res_item = vec![];
    for item in items {
        let path = item.file_info.path.clone();
        let size = item.file_info.metadata.len();

        if res_item.len() >= 10 {
            res.push(res_item);
            res_item = vec![];
            res_size = 0;
        }

        if size > max_size_bytes {
            trace!(?path, ?size, ?max_size_bytes, "File is too large");
            {
                failed.push((
                    path,
                    format!("file is too large: {} > {}", size, max_size_bytes),
                ));
            }
            continue;
        }

        if size + res_size > TelegramBot::max_payload_size().bytes().cast_unsigned() {
            res.push(res_item);
            res_size = 0;
            res_item = vec![];
        }

        res_item.push(item);
        res_size += size;
    }

    if !res_item.is_empty() {
        res.push(res_item);
    }

    (res, failed)
}

async fn infos_from_files<TFiles, TFile>(files: TFiles) -> (Vec<FileInfo>, Vec<(PathBuf, String)>)
where
    TFiles: IntoIterator<Item = TFile>,
    TFile: AsRef<Path>,
{
    let failed = Arc::new(Mutex::new(Vec::new()));

    let infos = files
        .into_iter()
        .map(|x| x.as_ref().to_path_buf())
        .map(|file_path| {
            let failed = failed.clone();

            async move {
                let mime = {
                    let file_path = file_path.clone();

                    tokio::task::spawn_blocking(move || infer_file_type(&file_path).ok())
                        .await
                        .ok()?
                };

                let metadata = match tokio::fs::metadata(&file_path).await {
                    Ok(meta) => Some(meta),
                    Err(e) => {
                        trace!(?e, "Failed to get metadata for file");
                        {
                            failed.lock().await.push((
                                file_path.clone(),
                                "failed to get metadata for file".to_string(),
                            ));
                        }

                        None
                    }
                }?;

                let dimensions = if let Some(mime) = &mime
                    && mime.type_() == "image"
                {
                    let info = app_helpers::ffprobe::ffprobe_async(&file_path).await.ok()?;
                    let stream = info.streams.first()?;

                    match (stream.width, stream.height) {
                        (Some(width), Some(height)) => Some(FileDimensions { width, height }),
                        _ => None,
                    }
                } else {
                    None
                };

                Some(FileInfo {
                    path: file_path,
                    mime,
                    metadata,
                    dimensions,
                })
            }
        })
        .collect::<FuturesUnordered<_>>()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    let failed = failed.lock().await.iter().cloned().collect();

    (infos, failed)
}
