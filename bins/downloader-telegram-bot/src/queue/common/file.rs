use std::{
    convert::Into,
    path::{Path, PathBuf},
};

use app_actions::fixers::{FixRequest, Fixer, handlers::file_extensions::FileExtension};
use app_helpers::{file_type::mime, id::time_thread_id};
use teloxide::{
    net::Download,
    prelude::*,
    types::{MediaKind, MessageKind, PhotoSize},
};
use tokio::fs::File;
use tracing::{debug, trace};

use crate::bot::TelegramBot;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileId(String);

impl FileId {
    pub fn from_message(message: &Message) -> Option<Self> {
        message.try_into().ok()
    }

    #[tracing::instrument]
    pub async fn download(&self, download_dir: &Path) -> Result<PathBuf, String> {
        debug!("Downloading file from telegram");

        let file_id = teloxide::types::FileId::from(self.0.clone());

        let f = TelegramBot::instance()
            .get_file(file_id.clone())
            .await
            .map_err(|e| format!("Error while getting file: {e:?}"))?;

        trace!("Got file: {:?}", f);

        let download_file_path = download_dir.join(format!(
            "{rand_id}.{id}.bin",
            rand_id = time_thread_id(),
            id = f.meta.unique_id
        ));

        trace!(
            "Downloading message file {:?} to: {:?}",
            &file_id, &download_file_path
        );

        let mut file = File::create(&download_file_path)
            .await
            .map_err(|e| format!("Error while creating file: {e:?}"))?;

        TelegramBot::pure_instance()
            .download_file(&f.path, &mut file)
            .await
            .map_err(|e| format!("Error while downloading file: {e:?}"))?;

        trace!("Downloaded file: {:?}", file);

        file.sync_all()
            .await
            .map_err(|e| format!("Error while syncing file: {e:?}"))?;

        trace!("Finished syncing file");

        trace!("Setting proper file extension");

        let final_file_path = FileExtension
            .run(&FixRequest::new(&download_file_path))
            .await
            .map(|x| x.file_path)
            .unwrap_or(download_file_path);

        debug!(path = ?final_file_path, "Downloaded file");

        Ok(final_file_path)
    }
}

impl std::fmt::Display for FileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<String> for FileId {
    fn from(x: String) -> Self {
        Self(x)
    }
}
impl From<teloxide::types::FileId> for FileId {
    fn from(x: teloxide::types::FileId) -> Self {
        Self(x.0)
    }
}
impl From<FileId> for teloxide::types::FileId {
    fn from(x: FileId) -> Self {
        Self(x.0)
    }
}
impl TryFrom<&Message> for FileId {
    type Error = FileIdError;

    fn try_from(value: &Message) -> Result<Self, Self::Error> {
        let px = |x: &PhotoSize| u64::from(x.width) * u64::from(x.height);

        let msg_data = match &value.kind {
            MessageKind::Common(x) => x,
            _ => return Err(FileIdError::UnknownMessageKind),
        };

        match &msg_data.media_kind {
            MediaKind::Video(x) => Ok(x.video.file.id.clone()),
            MediaKind::Animation(x) => Ok(x.animation.file.id.clone()),
            MediaKind::Audio(x) => Ok(x.audio.file.id.clone()),
            MediaKind::VideoNote(x) => Ok(x.video_note.file.id.clone()),
            MediaKind::Photo(x) if !x.photo.is_empty() => {
                let mut photos = x.photo.clone();
                photos.sort_unstable_by(|lt, gt| {
                    let pixels = px(gt).cmp(&px(lt));
                    if pixels != std::cmp::Ordering::Equal {
                        return pixels;
                    }

                    gt.width.cmp(&lt.width)
                });

                photos
                    .first()
                    .ok_or(FileIdError::PhotoMessageWithoutPhotos)
                    .map(|x| x.file.id.clone())
            }
            MediaKind::Document(x) => {
                let Some(mime_type) = &x.document.mime_type else {
                    return Err(FileIdError::DocumentMessageWithoutMimeType);
                };

                if !matches!(mime_type.type_().as_str(), "image" | "video" | "audio") {
                    return Err(FileIdError::UnsupportedDocumentType(mime_type.clone()));
                }

                Ok(x.document.file.id.clone())
            }
            _ => Err(FileIdError::UnsupportedMediaKind),
        }
        .map(Into::into)
    }
}
impl TryFrom<Message> for FileId {
    type Error = FileIdError;

    fn try_from(value: Message) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FileIdError {
    #[error("Unknown message kind")]
    UnknownMessageKind,
    #[error("Photo message without photos")]
    PhotoMessageWithoutPhotos,
    #[error("Document message without mime type")]
    DocumentMessageWithoutMimeType,
    #[error("Unsupported media kind")]
    UnsupportedMediaKind,
    #[error("Unsupported document type")]
    UnsupportedDocumentType(mime::Mime),
}
