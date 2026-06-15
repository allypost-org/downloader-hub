use std::fmt::Debug;

pub use common::{
    download_request::{DownloadRequest, DownloaderOptions},
    download_result::DownloadResult,
};
pub use handlers::DownloaderEntry;

mod common;
pub mod handlers;
mod helpers;

pub use handlers::AVAILABLE_DOWNLOADERS;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::config::ActionsConfig;

#[async_trait::async_trait]
#[typetag::serde(tag = "$downloader")]
pub trait Downloader: Debug + Send + Sync {
    fn name(&self) -> &'static str {
        self.typetag_name()
    }

    fn description(&self) -> &'static str;

    fn is_enabled(&self) -> bool {
        ActionsConfig::global().is_enabled(("downloader", self.name()))
    }

    async fn can_download(&self, request: &DownloadRequest) -> bool;

    async fn download(&self, req: &DownloadRequest) -> DownloaderReturn;
}

pub type DownloaderReturn = Result<DownloadResult, DownloaderError>;

#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type", content = "data")]
pub enum DownloaderError {
    #[error("Failed to download file: {0}")]
    FallibleFailed(String),
    #[error("Error downloading file: {0}")]
    Error(String),
}
impl DownloaderError {
    #[must_use]
    pub fn original_message(self) -> String {
        match self {
            Self::FallibleFailed(e) | Self::Error(e) => e,
        }
    }

    #[must_use]
    pub const fn is_soft_error(&self) -> bool {
        matches!(self, Self::FallibleFailed(_))
    }
}

pub async fn download_file(file: &DownloadRequest) -> DownloaderReturn {
    info!(?file, "Downloading file");

    let new_file_paths = download_file_with(&AVAILABLE_DOWNLOADERS, file).await;

    debug!("Downloaded files: {:?}", &new_file_paths);

    new_file_paths
}

pub async fn download_file_with(
    downloaders: &[DownloaderEntry],
    request: &DownloadRequest,
) -> DownloaderReturn {
    let downloader = find_downloader(downloaders, request).await.ok_or_else(|| {
        DownloaderError::FallibleFailed(format!(
            "Could not find a downloader that can handle {r:?}",
            r = request,
        ))
    })?;

    downloader.download(request).await
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
