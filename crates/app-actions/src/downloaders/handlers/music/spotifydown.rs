use std::{
    path::{Path, PathBuf},
    sync::LazyLock,
    time::Duration,
};

use app_helpers::domain::DomainParser;
use regex::Regex;
use serde::Deserialize;
use tracing::{debug, trace, warn};
use url::Url;

use super::Handler;
use crate::{
    common::request::Client,
    downloaders::{DownloadRequest, Downloader, handlers::generic::Generic},
};

const URL_BASE: &str = "https://spotifydown.com";
const API_BASE: &str = "https://api.spotifydown.com";
static PATH_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/track/(?<id>[a-zA-Z0-9]+)").expect("Invalid regex"));

#[derive(Debug)]
pub struct SpotifydownProvider;

#[async_trait::async_trait]
impl Handler for SpotifydownProvider {
    #[tracing::instrument(skip(self, song_url), fields(url = ?song_url.as_str()))]
    async fn download(&self, download_dir: &Path, song_url: &Url) -> anyhow::Result<PathBuf> {
        debug!("Downloading song");

        let download_url = Self::get_download_url(song_url).await.map_err(|e| {
            if let Some(e) = e.downcast_ref::<reqwest::Error>()
                && e.is_timeout()
            {
                warn!(
                    ?e,
                    "Timeout downloading song. Download provider may be down."
                );
                return anyhow::anyhow!("Timeout downloading song. Download provider may be down.");
            }
            warn!(?e, "Failed to download song");
            anyhow::anyhow!("Failed to download song from provider")
        })?;

        debug!(?download_url, "Download URL found. Downloading song.");

        Generic
            .download(&DownloadRequest::from_url(&download_url, download_dir))
            .await
            .map(|x| x.path)
            .map_err(|e| anyhow::anyhow!(e).context("Failed to download song"))
    }

    fn supports(&self, song_url: &Url) -> bool {
        let Some(root) = DomainParser::get_domain_root(song_url) else {
            return false;
        };

        root == "spotify.com"
    }

    // Disabled because site added captcha
    fn enabled(&self) -> bool {
        false
    }
}

impl SpotifydownProvider {
    pub async fn get_download_url(song_url: &Url) -> anyhow::Result<String> {
        let Some(track_id) = PATH_REGEX
            .captures(song_url.path())
            .and_then(|x| x.name("id"))
        else {
            anyhow::bail!("Invalid Spotify URL");
        };

        trace!(?track_id, "Got track ID from song URL");

        let api_url = format!("{API_BASE}/download/{id}", id = track_id.as_str());
        trace!(?api_url, "Got API URL for song download request");
        let res = Client::base()
            .map_err(|e| anyhow::anyhow!(e))?
            .get(api_url)
            .timeout(Duration::from_secs(5))
            .header("origin", URL_BASE)
            .header("referer", URL_BASE)
            .send()
            .await?
            .json::<DownloadResponse>()
            .await?;

        trace!(?res, "Got download response");

        match res {
            DownloadResponse::Error { message } => {
                Err(anyhow::anyhow!(message).context("Failed to get song download link"))
            }

            DownloadResponse::Success { link } => Ok(link),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DownloadResponse {
    Success { link: String },
    Error { message: String },
}
