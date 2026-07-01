use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use app_requests::UrlWithMeta;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    common::fallible::TaskFallible,
    downloaders::DownloaderEntry,
    extractors::{ExtractedInfo, ExtractedUrlInfo},
};

pub type DownloaderOptions = HashMap<String, serde_json::Value>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRequest {
    pub url: UrlWithMeta,
    pub download_dir: PathBuf,
    pub preferred_downloader: Option<DownloaderEntry>,
    pub downloader_options: DownloaderOptions,
    pub fallible: TaskFallible,
}
impl DownloadRequest {
    #[must_use]
    pub fn from_url<U>(url: U, download_dir: &Path) -> Self
    where
        U: Into<UrlWithMeta>,
    {
        Self {
            url: url.into(),
            download_dir: download_dir.to_path_buf(),
            preferred_downloader: None,
            downloader_options: HashMap::new(),
            fallible: TaskFallible::default(),
        }
    }

    #[must_use]
    pub fn from_extracted_url(info: &ExtractedUrlInfo, download_dir: &Path) -> Self {
        Self {
            url: info.url.clone(),
            download_dir: download_dir.to_path_buf(),
            preferred_downloader: info.preferred_downloader.clone(),
            downloader_options: info.downloader_options.clone(),
            fallible: info.fallible,
        }
    }

    #[must_use]
    pub fn from_extracted_info(info: &ExtractedInfo, download_dir: &Path) -> Vec<Self> {
        info.urls
            .iter()
            .map(|x| Self::from_extracted_url(x, download_dir))
            .collect()
    }

    #[must_use]
    pub fn is_preferred_downloader(&self, name: &'static str) -> bool {
        self.preferred_downloader
            .as_ref()
            .is_some_and(|x| x.name() == name)
    }

    #[must_use]
    pub fn has_preferred_downloader(&self) -> bool {
        self.preferred_downloader.is_some()
    }

    #[must_use]
    pub const fn fallibility(&self) -> TaskFallible {
        self.fallible
    }
}
impl DownloadRequest {
    #[must_use]
    pub const fn download_dir(&self) -> &PathBuf {
        &self.download_dir
    }

    #[must_use]
    pub fn with_preferred_downloader<D>(mut self, downloader: DownloaderEntry) -> Self {
        self.preferred_downloader = Some(downloader);
        self
    }

    #[must_use]
    pub fn with_downloader_option<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Into<serde_json::Value>,
    {
        self.downloader_options.insert(key.into(), value.into());
        self
    }

    #[must_use]
    pub fn with_downloader_options<T>(mut self, options: T) -> Self
    where
        T: Into<DownloaderOptions>,
    {
        self.downloader_options = options.into();
        self
    }

    #[must_use]
    pub fn with_downloader_options_if<F, T>(self, name: &'static str, options: F) -> Self
    where
        F: FnOnce() -> T,
        T: Into<DownloaderOptions>,
    {
        if self.is_preferred_downloader(name) {
            self.with_downloader_options(options())
        } else {
            self
        }
    }

    #[must_use]
    pub fn downloader_option_raw(&self, key: &str) -> Option<&serde_json::Value> {
        self.downloader_options.get(key)
    }

    #[must_use]
    pub fn downloader_option<T>(&self, key: &str) -> Option<T>
    where
        T: DeserializeOwned,
    {
        let val = self.downloader_options.get(key)?.clone();

        serde_json::from_value(val).ok()
    }

    #[must_use]
    pub fn downloader_options<T>(&self) -> Option<T>
    where
        T: DeserializeOwned,
    {
        let val = match serde_json::to_value(self.downloader_options.clone()) {
            Ok(x) => x,
            Err(e) => {
                tracing::warn!(?e, "Failed to serialize downloader options");
                return None;
            }
        };

        match serde_json::from_value(val) {
            Ok(x) => Some(x),
            Err(e) => {
                tracing::warn!(?e, "Failed to deserialize downloader options");
                None
            }
        }
    }
}
