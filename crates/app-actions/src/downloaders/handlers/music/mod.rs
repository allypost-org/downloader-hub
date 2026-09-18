pub mod spotifydown;
pub mod yams;

use std::{path::PathBuf, sync::LazyLock};

use serde::{Deserialize, Serialize};
use tracing::warn;
use url::Url;

use super::{DownloadRequest, Downloader, DownloaderReturn};
use crate::{
    ActionCtx,
    downloaders::{DownloadResult, DownloaderError},
};

static HANDLERS: LazyLock<Vec<DownloadHandler>> = LazyLock::new(|| {
    vec![
        DownloadHandler::new(yams::YamsProvider),
        DownloadHandler::new(spotifydown::SpotifydownProvider),
    ]
    .into_iter()
    .filter(DownloadHandler::enabled)
    .collect()
});

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Music;

#[async_trait::async_trait]
#[typetag::serde]
impl Downloader for Music {
    fn description(&self) -> &'static str {
        "Download songs from Spotify, Deezer, Tidal, and various other music providers. Depends on \
         external services so may be randomly unavailable."
    }

    async fn can_download(&self, request: &DownloadRequest) -> bool {
        Self::supports(request.url.url())
    }

    async fn download(&self, ctx: &ActionCtx, req: &DownloadRequest) -> DownloaderReturn {
        let song_url = req.url.url();

        for handler in HANDLERS.iter() {
            if !handler.supports(song_url) {
                continue;
            }

            match handler.download(ctx, req).await {
                Ok(path) => {
                    return Ok(DownloadResult {
                        path,
                        request: req.clone(),
                    });
                }
                Err(e) => {
                    warn!(?e, "Failed to download song");
                    if let Some(error @ DownloaderError::ExceedsMaxFilesize { .. }) =
                        e.downcast_ref::<DownloaderError>()
                    {
                        return Err(error.clone());
                    }
                }
            }
        }

        Err(DownloaderError::Error(
            "No handler succeeded for song".to_string(),
        ))
    }
}

impl Music {
    pub fn supports(song_url: &Url) -> bool {
        HANDLERS.iter().any(|handler| handler.supports(song_url))
    }
}

#[derive(Debug)]
struct DownloadHandler {
    provider: Box<dyn Handler>,
}
impl DownloadHandler {
    fn new<T>(provider: T) -> Self
    where
        T: Handler + 'static,
    {
        Self {
            provider: Box::new(provider),
        }
    }

    #[must_use]
    pub fn supports(&self, url: &Url) -> bool {
        self.provider.supports(url)
    }

    #[must_use]
    pub fn enabled(&self) -> bool {
        self.provider.enabled()
    }

    pub async fn download(
        &self,
        ctx: &ActionCtx,
        request: &DownloadRequest,
    ) -> Result<PathBuf, anyhow::Error> {
        self.provider.download(ctx, request).await
    }
}

#[async_trait::async_trait]
trait Handler: std::fmt::Debug + Send + Sync {
    async fn download(&self, ctx: &ActionCtx, request: &DownloadRequest)
    -> anyhow::Result<PathBuf>;

    fn supports(&self, song_url: &Url) -> bool;

    fn enabled(&self) -> bool {
        true
    }
}
