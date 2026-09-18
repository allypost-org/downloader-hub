use std::string::ToString;

use serde::{Deserialize, Serialize};
use tracing::trace;
use url::Url;

use super::{ExtractInfoRequest, ExtractedInfo, Extractor};
use crate::ActionCtx;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Imgur;

#[async_trait::async_trait]
#[typetag::serde]
impl Extractor for Imgur {
    fn description(&self) -> &'static str {
        "Gets images and other media from imgur posts"
    }

    async fn can_handle(&self, request: &ExtractInfoRequest) -> bool {
        Self::is_media_url(&request.url) || Self::is_post_url(&request.url)
    }

    async fn extract_info(
        &self,
        ctx: &ActionCtx,
        request: &ExtractInfoRequest,
    ) -> Result<ExtractedInfo, String> {
        let post_data = get_post_data(ctx, request).await?;

        let media = post_data.media.into_iter().map(|x| x.url);

        Ok(ExtractedInfo::from_urls(request, media))
    }
}

#[must_use]
pub fn is_imgur_direct_media_url(url: &str) -> bool {
    url.starts_with("https://i.imgur.com/")
}

#[must_use]
pub fn is_imgur_url(url: &str) -> bool {
    url.starts_with("https://imgur.com/") || url.starts_with("http://imgur.com/")
}

impl Imgur {
    #[must_use]
    pub fn is_media_url(url: &Url) -> bool {
        url.host_str().is_some_and(|x| x == "i.imgur.com")
    }

    #[must_use]
    pub fn is_post_url(url: &Url) -> bool {
        let host = url.host_str();

        host.is_some_and(|x| x == "imgur.com") || host.is_some_and(|x| x == "www.imgur.com")
    }
}

#[derive(Debug, Deserialize)]
struct ImgurPostData {
    pub media: Vec<ImgurPostMedia>,
}

#[derive(Debug, Deserialize)]
struct ImgurPostMedia {
    url: String,
}

async fn get_post_data(ctx: &ActionCtx, req: &ExtractInfoRequest) -> Result<ImgurPostData, String> {
    let resp = req
        .as_request_builder(ctx)?
        .send()
        .await
        .map_err(|e| format!("Failed to send request to imgur: {:?}", e))?
        .text()
        .await
        .map_err(|e| format!("Failed to get text from imgur response: {:?}", e))?;

    trace!("Got response from imgur");

    let script_data = tokio::task::spawn_blocking(move || {
        let dom = tl::parse(&resp, tl::ParserOptions::default())
            .map_err(|e| format!("Failed to parse html from imgur: {:?}", e))?;
        let parser = dom.parser();

        trace!("Parsed html from imgur");

        dom.query_selector("script")
            .expect("Failed parse query selector")
            .filter_map(|x| x.get(parser))
            .filter_map(|x| x.as_tag())
            .find_map(|x| {
                x.inner_text(parser)
                    .trim()
                    .strip_prefix("window.postDataJSON=")
                    .map(ToString::to_string)
            })
            .ok_or_else(|| "Failed to get script data".to_string())
    })
    .await
    .map_err(|e| format!("Failed to get script data from imgur: {:?}", e))??;

    trace!(script_data, "Got script data from imgur");

    // The replace is required because Imgur improperly always escapes single quotes
    serde_json::from_str::<String>(&script_data.replace("\\'", "'"))
        .or_else(|_| serde_json::from_str::<String>(&script_data))
        .and_then(|x| serde_json::from_str::<ImgurPostData>(&x))
        .map_err(|e| format!("Failed to deserialize script data from imgur: {:?}", e))
}
