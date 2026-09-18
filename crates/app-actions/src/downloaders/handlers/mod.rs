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

fn all_downloaders() -> Vec<DownloaderEntry> {
    vec![
        Arc::new(yt_dlp::YtDlp),
        Arc::new(generic::Generic),
        Arc::new(music::Music),
    ]
}
