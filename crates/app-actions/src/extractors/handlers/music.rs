use serde::{Deserialize, Serialize};

use super::{ExtractInfoRequest, ExtractedInfo, Extractor};
use crate::{ActionCtx, downloaders::handlers::music::Music as MusicDownloader};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Music;

#[async_trait::async_trait]
#[typetag::serde]
impl Extractor for Music {
    fn description(&self) -> &'static str {
        "Download songs from Spotify, Deezer, Tidal, and various other music providers. Depends on \
         external services so may be randomly unavailable."
    }

    async fn can_handle(&self, request: &ExtractInfoRequest) -> bool {
        MusicDownloader::supports(&request.url)
    }

    async fn extract_info(
        &self,
        _ctx: &ActionCtx,
        request: &ExtractInfoRequest,
    ) -> Result<ExtractedInfo, String> {
        Ok(ExtractedInfo::from_url(request, request.url.as_str())
            .with_preferred_downloader(Some(MusicDownloader)))
    }
}
