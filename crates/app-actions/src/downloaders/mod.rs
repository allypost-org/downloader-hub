use std::fmt::Debug;

pub use common::{
    download_request::{DownloadRequest, DownloaderOptions},
    download_result::DownloadResult,
};
pub use handlers::DownloaderEntry;

mod common;
pub mod handlers;
mod helpers;

use app_config::{EntryCategory, EntryId};
pub use handlers::ALL_DOWNLOADERS;
use serde::{Deserialize, Serialize};

#[async_trait::async_trait]
#[typetag::serde(tag = "$downloader")]
pub trait Downloader: Debug + Send + Sync {
    fn name(&self) -> &'static str {
        self.typetag_name()
    }

    fn entry_id(&self) -> EntryId {
        EntryId::new(EntryCategory::Downloader, self.name())
    }

    fn description(&self) -> &'static str;

    async fn can_download(&self, request: &DownloadRequest) -> bool;

    async fn download(&self, ctx: &crate::ActionCtx, req: &DownloadRequest) -> DownloaderReturn;
}

impl app_config::AsEntryId for dyn Downloader {
    fn entry_id(&self) -> EntryId {
        Downloader::entry_id(self)
    }
}

pub type DownloaderReturn = Result<DownloadResult, DownloaderError>;

#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type", content = "data")]
pub enum DownloaderError {
    #[error("Failed to download file: {0}")]
    FallibleFailed(String),
    #[error("Error downloading file: {0}")]
    Error(String),
    #[error("Source exceeds configured max-filesize limit of {limit}")]
    ExceedsMaxFilesize { limit: size::Size },
}
impl DownloaderError {
    #[must_use]
    pub fn original_message(self) -> String {
        match self {
            Self::FallibleFailed(e) | Self::Error(e) => e,
            Self::ExceedsMaxFilesize { limit } => {
                format!("source exceeds configured max-filesize limit of {limit}")
            }
        }
    }

    #[must_use]
    pub const fn is_soft_error(&self) -> bool {
        matches!(self, Self::FallibleFailed(_))
    }

    #[must_use]
    pub const fn is_max_filesize(&self) -> bool {
        matches!(self, Self::ExceedsMaxFilesize { .. })
    }
}

pub async fn download_file_with(
    ctx: &crate::ActionCtx,
    downloaders: &[DownloaderEntry],
    request: &DownloadRequest,
) -> DownloaderReturn {
    let downloader = find_downloader(downloaders, request).await.ok_or_else(|| {
        DownloaderError::FallibleFailed(format!(
            "Could not find a downloader that can handle {r:?}",
            r = request,
        ))
    })?;

    downloader.download(ctx, request).await
}

async fn find_downloader(
    downloaders: &[DownloaderEntry],
    request: &DownloadRequest,
) -> Option<DownloaderEntry> {
    if let Some(downloader) = &request.preferred_downloader
        && downloader.can_download(request).await
    {
        return Some(downloader.clone());
    }

    for downloader in downloaders {
        if downloader.can_download(request).await {
            return Some(downloader.clone());
        }
    }

    None
}
