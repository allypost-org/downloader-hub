pub mod generic;
pub mod music;
pub mod yt_dlp;

use std::sync::{Arc, LazyLock};

pub use super::{
    Downloader, DownloaderError, DownloaderReturn,
    common::{download_request::DownloadRequest, download_result::DownloadResult},
};

pub type DownloaderEntry = Arc<dyn Downloader>;

pub static ALL_DOWNLOADERS: LazyLock<Vec<DownloaderEntry>> = LazyLock::new(all_downloaders);

pub static AVAILABLE_DOWNLOADERS: LazyLock<Vec<DownloaderEntry>> =
    LazyLock::new(available_downloaders);

fn all_downloaders() -> Vec<DownloaderEntry> {
    vec![
        Arc::new(yt_dlp::YtDlp),
        Arc::new(generic::Generic),
        Arc::new(music::Music),
    ]
}

#[must_use]
fn available_downloaders() -> Vec<DownloaderEntry> {
    all_downloaders()
        .into_iter()
        .filter(|x| x.is_enabled())
        .collect()
}
