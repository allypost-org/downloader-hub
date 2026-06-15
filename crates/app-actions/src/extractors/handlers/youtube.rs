use serde::{Deserialize, Serialize};

use super::{ExtractInfoRequest, ExtractedInfo, Extractor};
use crate::downloaders::handlers::yt_dlp::YtDlp;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Youtube;

#[async_trait::async_trait]
#[typetag::serde]
impl Extractor for Youtube {
    fn description(&self) -> &'static str {
        "YouTube extractor. Extracts video information from YouTube via yt-dlp."
    }

    async fn can_handle(&self, request: &ExtractInfoRequest) -> bool {
        static YT_DOMAINS: [&str; 4] = ["youtube.com", "youtu.be", ".youtube.com", ".youtu.be"];

        let Some(domain) = request.url.domain() else {
            return false;
        };

        YT_DOMAINS.contains(&domain) || YT_DOMAINS.iter().any(|x| domain.ends_with(x))
    }

    async fn extract_info(&self, request: &ExtractInfoRequest) -> Result<ExtractedInfo, String> {
        Ok(ExtractedInfo::from_url(request, request.url.as_str())
            .with_preferred_downloader(Some(YtDlp)))
    }
}
