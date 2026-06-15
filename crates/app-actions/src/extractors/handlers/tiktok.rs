use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::{debug, trace};
use url::Url;

use super::{ExtractInfoRequest, ExtractedInfo, Extractor};
use crate::{
    common::url::UrlWithMeta, config::ActionsConfig, downloaders::handlers::generic::Generic,
};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Tiktok;

#[async_trait::async_trait]
#[typetag::serde]
impl Extractor for Tiktok {
    fn description(&self) -> &'static str {
        "Get videos from TikTok posts"
    }

    async fn can_handle(&self, request: &ExtractInfoRequest) -> bool {
        Self::is_post_url(&request.url)
    }

    async fn extract_info(&self, request: &ExtractInfoRequest) -> Result<ExtractedInfo, String> {
        let media_urls = get_media_download_urls(request)
            .await
            .map_err(|e| format!("Failed to get media download urls for tiktok post: {:?}", e))?;

        Ok(ExtractedInfo::from_url(request, media_urls).with_preferred_downloader(Some(Generic)))
    }
}

impl Tiktok {
    #[must_use]
    pub fn is_post_url(url: &Url) -> bool {
        url.host_str().is_some_and(|x| x == "www.tiktok.com") && url.path().starts_with("/@")
    }
}

async fn get_media_download_urls(req: &ExtractInfoRequest) -> Result<UrlWithMeta, String> {
    debug!("Getting media download urls for tiktok post");

    let resp = req
        .as_request_builder()?
        .send()
        .await
        .map_err(|e| format!("Failed to send request to tiktok: {:?}", e))?;
    trace!(?resp, "Got response from tiktok");

    if !resp.status().is_success() {
        return Err(format!(
            "Got non-success status code from TikTok: {:?}",
            resp.status()
        ));
    }

    let mut resp_cookies = HashMap::<String, String>::new();
    for cookie in resp.cookies() {
        resp_cookies.insert(cookie.name().to_string(), cookie.value().to_string());
    }
    trace!(?resp_cookies, "Got cookies from tiktok response");

    let csrf_token = resp_cookies
        .get("tt_chain_token")
        .ok_or_else(|| "Failed to get csrf token from response cookies".to_string())?;
    trace!(?csrf_token, "Got csrf token from response cookies");

    let resp_body = resp
        .text()
        .await
        .map_err(|e| format!("Failed to get response body: {:?}", e))?;
    debug!("Got response body from tiktok");

    let post_data = tokio::task::spawn_blocking(move || {
        let dom = tl::parse(&resp_body, tl::ParserOptions::default())
            .map_err(|e| format!("Failed to parse response body: {:?}", e))?;
        trace!("Parsed response body as HTML");
        let parser = dom.parser();
        let data_el = dom
            .get_element_by_id("__UNIVERSAL_DATA_FOR_REHYDRATION__")
            .ok_or_else(|| {
                "Failed to find element with id __UNIVERSAL_DATA_FOR_REHYDRATION__ in response body"
                    .to_string()
            })?
            .get(parser)
            .ok_or_else(|| {
                "Failed to get element with id __UNIVERSAL_DATA_FOR_REHYDRATION__ in response body"
                    .to_string()
            })?;

        let data_el_text = data_el.inner_text(parser).to_string();

        serde_json::from_str::<serde_json::Value>(&data_el_text).map_err(|e| {
            format!(
                "Failed to parse post data from element with id \
                 __UNIVERSAL_DATA_FOR_REHYDRATION__: {:?}",
                e
            )
        })
    })
    .await
    .map_err(|e| format!("Failed to get post data from response body: {:?}", e))??;

    trace!("Got post data from response body");

    let video_data = post_data
        .get("__DEFAULT_SCOPE__")
        .and_then(|x| x.get("webapp.video-detail"))
        .and_then(|x| x.get("itemInfo"))
        .and_then(|x| x.get("itemStruct"))
        .ok_or_else(|| "Failed to get video data from post data".to_string())?;
    trace!(?video_data, "Got video data from post data");

    let video_url = video_data
        .get("video")
        .and_then(|x| x.get("playAddr"))
        .and_then(|x| x.as_str())
        .ok_or_else(|| "Failed to get video url from video data".to_string())?;
    trace!(?video_url, "Got video url from video data");

    let download_info = UrlWithMeta::from_url_str(video_url)
        .with_header("User-Agent", &ActionsConfig::request().user_agent)
        .with_header("Referer", &req.url)
        .with_header("Cookie", format!("tt_chain_token={}", csrf_token));

    Ok(download_info)
}
