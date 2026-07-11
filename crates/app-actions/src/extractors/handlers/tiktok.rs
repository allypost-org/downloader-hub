use std::{collections::HashMap, sync::LazyLock};

use app_requests::{Client, UrlWithMeta, reqwest::Response};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::{debug, trace};
use url::Url;

use super::{ExtractInfoRequest, ExtractedInfo, Extractor};
use crate::{config::ActionsConfig, downloaders::handlers::generic::Generic};

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

        Ok(ExtractedInfo::from_urls(request, media_urls).with_preferred_downloader(Some(Generic)))
    }
}

impl Tiktok {
    #[must_use]
    pub fn is_post_url(url: &Url) -> bool {
        url.host_str().is_some_and(|x| x == "vm.tiktok.com")
            || (url.host_str().is_some_and(|x| x == "www.tiktok.com")
                && url.path().starts_with("/@"))
    }
}

struct TiktokPage {
    final_url: Url,
    cookies: HashMap<String, String>,
    post_data: serde_json::Value,
}

static ITEM_ID_MATCH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/(?:photo|video)/(?P<item_id>\d+)").expect("Invalid regex"));

async fn get_media_download_urls(req: &ExtractInfoRequest) -> Result<Vec<UrlWithMeta>, String> {
    debug!("Getting media download urls for tiktok post");

    let page = fetch_page(req).await?;
    trace!(?page.final_url, "Fetched tiktok page");

    if let Ok(media_urls) =
        media_urls_from_post_data(&page.post_data, &page.final_url, &page.cookies)
    {
        return Ok(media_urls);
    }

    debug!(
        ?page.final_url,
        "Primary tiktok extraction failed, trying video-page fallback"
    );
    let fallback_url = video_page_fallback_url(&page.final_url)?;
    let fallback_page = fetch_page_at(&fallback_url, req, &page.cookies).await?;
    media_urls_from_post_data(
        &fallback_page.post_data,
        &fallback_page.final_url,
        &fallback_page.cookies,
    )
}

async fn fetch_page(req: &ExtractInfoRequest) -> Result<TiktokPage, String> {
    let resp = req
        .as_request_builder()?
        .send()
        .await
        .map_err(|e| format!("Failed to send request to tiktok: {:?}", e))?;
    parse_page_response(resp).await
}

async fn fetch_page_at(
    url: &Url,
    req: &ExtractInfoRequest,
    cookies: &HashMap<String, String>,
) -> Result<TiktokPage, String> {
    let mut builder = Client::base()
        .map_err(|e| format!("Failed to create client: {:?}", e))?
        .get(url.as_str());

    if !req.headers.contains_key(http::header::USER_AGENT) {
        builder = builder.header(
            http::header::USER_AGENT,
            ActionsConfig::request().user_agent.as_str(),
        );
    }

    for (k, v) in &req.headers {
        builder = builder.header(k, v);
    }

    if !cookies.is_empty() {
        builder = builder.header(http::header::COOKIE, cookie_header(cookies));
    }

    let resp = builder
        .send()
        .await
        .map_err(|e| format!("Failed to send fallback request to tiktok: {:?}", e))?;
    parse_page_response(resp).await
}

async fn parse_page_response(resp: Response) -> Result<TiktokPage, String> {
    trace!(?resp, "Got response from tiktok");

    if !resp.status().is_success() {
        return Err(format!(
            "Got non-success status code from TikTok: {:?}",
            resp.status()
        ));
    }

    let final_url = resp.url().clone();
    let cookies = collect_cookies(&resp);
    trace!(?cookies, "Got cookies from tiktok response");

    let resp_body = resp
        .text()
        .await
        .map_err(|e| format!("Failed to get response body: {:?}", e))?;
    debug!("Got response body from tiktok");

    let post_data = tokio::task::spawn_blocking(move || parse_post_data_from_html(&resp_body))
        .await
        .map_err(|e| format!("Failed to get post data from response body: {:?}", e))??;

    trace!("Got post data from response body");

    Ok(TiktokPage {
        final_url,
        cookies,
        post_data,
    })
}

fn collect_cookies(resp: &Response) -> HashMap<String, String> {
    let mut cookies = HashMap::<String, String>::new();
    for cookie in resp.cookies() {
        cookies.insert(cookie.name().to_string(), cookie.value().to_string());
    }
    cookies
}

fn cookie_header(cookies: &HashMap<String, String>) -> String {
    cookies
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

fn parse_post_data_from_html(resp_body: &str) -> Result<serde_json::Value, String> {
    let dom = tl::parse(resp_body, tl::ParserOptions::default())
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
            "Failed to parse post data from element with id __UNIVERSAL_DATA_FOR_REHYDRATION__: \
             {:?}",
            e
        )
    })
}

fn media_urls_from_post_data(
    post_data: &serde_json::Value,
    referer: &Url,
    cookies: &HashMap<String, String>,
) -> Result<Vec<UrlWithMeta>, String> {
    let video_data = post_data
        .get("__DEFAULT_SCOPE__")
        .and_then(|x| x.get("webapp.video-detail"))
        .and_then(|x| x.get("itemInfo"))
        .and_then(|x| x.get("itemStruct"))
        .ok_or_else(|| "Failed to get video data from post data".to_string())?;
    trace!(?video_data, "Got video data from post data");

    if let Some(video_url) = video_data
        .get("video")
        .and_then(|x| x.get("playAddr"))
        .and_then(|x| x.as_str())
        .filter(|url| !url.is_empty())
    {
        trace!(?video_url, "Got video url from video data");
        return Ok(vec![download_info(video_url, referer, cookies)]);
    }

    let image_urls = video_data
        .get("imagePost")
        .and_then(|x| x.get("images"))
        .and_then(|x| x.as_array())
        .map(|images| {
            images
                .iter()
                .filter_map(|image| {
                    image
                        .get("imageURL")
                        .and_then(|x| x.get("urlList"))
                        .and_then(|x| x.as_array())
                        .and_then(|x| x.first())
                        .and_then(|x| x.as_str())
                })
                .map(|url| download_info(url, referer, cookies))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if image_urls.is_empty() {
        return Err("Failed to get media urls from post data".to_string());
    }

    trace!(count = image_urls.len(), "Got image urls from post data");
    Ok(image_urls)
}

fn video_page_fallback_url(final_url: &Url) -> Result<Url, String> {
    let item_id = ITEM_ID_MATCH
        .captures(final_url.path())
        .and_then(|caps| caps.name("item_id"))
        .map(|m| m.as_str())
        .ok_or_else(|| {
            format!(
                "Failed to extract item id from tiktok url for fallback: {:?}",
                final_url
            )
        })?;

    let mut fallback_url = final_url.clone();
    let path = final_url.path().replace("/photo/", "/video/");
    let fallback_path = if path.contains("/video/") {
        path
    } else {
        format!("{}/video/{}", final_url.path(), item_id)
    };

    fallback_url.set_path(&fallback_path);
    fallback_url.set_query(None);
    fallback_url.set_fragment(None);

    Ok(fallback_url)
}

fn download_info(media_url: &str, referer: &Url, cookies: &HashMap<String, String>) -> UrlWithMeta {
    let mut download_info = UrlWithMeta::from_url_str(media_url)
        .with_header("User-Agent", &ActionsConfig::request().user_agent)
        .with_header("Referer", referer.as_str());

    if let Some(csrf_token) = cookies.get("tt_chain_token") {
        download_info = download_info.with_header("Cookie", format!("tt_chain_token={csrf_token}"));
    }

    download_info
}
