use serde::{Deserialize, Serialize};

use super::{ExtractInfoRequest, ExtractedInfo, Extractor};
use crate::{
    ActionCtx,
    downloaders::handlers::{generic::Generic, yt_dlp::YtDlp},
};

#[must_use]
pub fn is_reddit_image_url(url: &str) -> bool {
    url.starts_with("https://i.redd.it/")
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Reddit;

#[async_trait::async_trait]
#[typetag::serde]
impl Extractor for Reddit {
    fn description(&self) -> &'static str {
        "Gets reddit media. Only works on media links (eg. https://i.redd.it/...)"
    }

    async fn can_handle(&self, request: &ExtractInfoRequest) -> bool {
        Self::is_media_url(request.url.as_str())
    }

    async fn extract_info(
        &self,
        _ctx: &ActionCtx,
        request: &ExtractInfoRequest,
    ) -> Result<ExtractedInfo, String> {
        let url = {
            let mut x = request.url.clone();
            if x.query_pairs().all(|(k, _)| k != "s") {
                let _ = x.set_host(Some("i.redd.it"));
                x.set_query(None);
            }
            x
        };
        let file_ext = url.path().split('.').next_back().unwrap_or_default();
        let info = {
            let x = ExtractedInfo::from_url(request, url.as_str());
            match file_ext {
                "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "tiff" | "tif" | "ico" => {
                    x.with_preferred_downloader(Some(Generic))
                }
                _ => x.with_preferred_downloader(Some(YtDlp)),
            }
        };

        Ok(info)
    }
}

impl Reddit {
    #[must_use]
    pub fn is_media_url(url: &str) -> bool {
        url.starts_with("https://i.redd.it/") || url.starts_with("https://preview.redd.it/")
    }
}
