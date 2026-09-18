use std::{result::Result, sync::LazyLock, time::Duration};

use app_requests::Client;
use http::{StatusCode, header};
use regex::Regex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use tracing::{debug, trace};
use url::Url;

use super::{ExtractInfoRequest, Extractor};
use crate::{ActionCtx, extractors::ExtractedInfo};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Instagram;

#[async_trait::async_trait]
#[typetag::serde]
impl Extractor for Instagram {
    fn description(&self) -> &'static str {
        "Get images and videos from Instagram posts"
    }

    async fn can_handle(&self, request: &ExtractInfoRequest) -> bool {
        Self::is_post_url(&request.url)
    }

    async fn extract_info(
        &self,
        ctx: &ActionCtx,
        request: &ExtractInfoRequest,
    ) -> Result<ExtractedInfo, String> {
        let cookie = request
            .headers
            .get(header::COOKIE)
            .and_then(|v| v.to_str().ok());

        for i in 0_u32..3 {
            match get_media_urls(ctx, request.url.as_str(), cookie).await? {
                Some(media_urls) => return Ok(ExtractedInfo::from_urls(request, media_urls)),
                None => {
                    let delay = Duration::from_secs(2_u64.pow(i));
                    debug!(
                        ?i,
                        ?delay,
                        "Failed to get media urls from post, retrying after delay",
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }

        Err(
            "Failed to get media urls from post after 3 attempts. Post is probably age restricted \
             or we hit a rate limit."
                .to_string(),
        )
    }
}

static URL_MATCH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^https?://(www\.)?instagram\.com/(p|reels?)/(?P<post_id>[^/?]+)")
        .expect("Invalid regex")
});

impl Instagram {
    pub fn is_post_url(url: &Url) -> bool {
        URL_MATCH.is_match(url.as_str())
    }
}

async fn get_media_urls(
    ctx: &ActionCtx,
    url: &str,
    cookie: Option<&str>,
) -> Result<Option<Vec<Url>>, String> {
    if let Some(cookie) = cookie {
        match get_media_urls_authed(ctx, url, cookie).await {
            Ok(Some(urls)) => return Ok(Some(urls)),
            Ok(None) => debug!("Authed fetch returned no media; falling back to anonymous"),
            Err(e) => debug!(?e, "Authed extraction failed; falling back to anonymous"),
        }
    }
    get_media_urls_anonymous(ctx, url).await
}

#[tracing::instrument(skip_all, fields(url = %url))]
async fn get_media_urls_authed(
    ctx: &ActionCtx,
    url: &str,
    cookie: &str,
) -> Result<Option<Vec<Url>>, String> {
    trace!("Fetching instagram media URLs from post (authed)");

    let Some(shortcode) = shortcode_from_url(url) else {
        return Err("Failed to extract shortcode from URL".to_string());
    };

    let client = Client::sneaky().map_err(|e| format!("Failed to create client: {e:?}"))?;

    let resp = client
        .get(url)
        .header(header::USER_AGENT, ctx.request.user_agent.as_str())
        .header(header::COOKIE, cookie)
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {e:?}"))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to get response: {:?}", resp.status()));
    }

    let resp_html = resp
        .text()
        .await
        .map_err(|e| format!("Failed to get response text: {e:?}"))?;

    let info =
        tokio::task::spawn_blocking(move || extract_info_from_html_authed(&resp_html, &shortcode))
            .await
            .map_err(|e| format!("Instagram authed extraction crashed: {e:?}"))?;

    Ok(info)
}

fn shortcode_from_url(url: &str) -> Option<String> {
    URL_MATCH
        .captures(url)
        .and_then(|captures| captures.name("post_id"))
        .map(|m| m.as_str().to_string())
}

#[tracing::instrument(skip_all, fields(url = %url))]
async fn get_media_urls_anonymous(ctx: &ActionCtx, url: &str) -> Result<Option<Vec<Url>>, String> {
    trace!("Fetching instagram media URLs from post");

    let client = Client::sneaky().map_err(|e| format!("Failed to create client: {e:?}"))?;

    let resp = client
        .get(url)
        .header(header::USER_AGENT, ctx.request.user_agent.as_str())
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {e:?}"))?;

    trace!(?resp, "Got response from post");

    if resp.status() == StatusCode::FORBIDDEN {
        return Err(
            "Instagram returned 403. This usually means that the request is being rate limited. \
             Try again later."
                .to_string(),
        );
    }

    if resp.status().is_redirection() {
        return Err(format!(
            "Instagram returned redirect ({:?}). This usually means that the post is not \
             available (private, deleted, etc.).",
            resp.status()
        ));
    }

    if !resp.status().is_success() {
        return Err(format!("Failed to get response: {:?}", resp.status()));
    }

    debug!("Got successful response from post, extracting info");

    let resp_html = resp
        .text()
        .await
        .map_err(|e| format!("Failed to get response text: {e:?}"))?;

    trace!(len = resp_html.len(), "Got response text from post");

    let info = tokio::task::spawn_blocking(move || extract_info_from_html(&resp_html))
        .await
        .map_err(|e| format!("Instagram extraction crashed: {e:?}"))?;

    trace!(?info, "Extracted info from post");

    let Some(info) = info else {
        return Ok(None);
    };

    let urls = info.get_media_urls();

    debug!(?urls, "Found media urls in post");

    Ok(Some(urls))
}

#[derive(Debug, Deserialize)]
#[allow(clippy::enum_variant_names)]
enum InstagramStreamCache {
    #[serde(rename = "xig_polaris_media")]
    Media {
        #[serde(rename = "if_not_gated_logged_out")]
        resource: InstagramMedia,
    },
    #[serde(rename = "xdt_api__v1__clips__clips_on_logged_out_connection_v2")]
    Clip { edges: Vec<ClipEdge> },
}
impl InstagramStreamCache {
    fn get_media_urls(&self) -> Vec<Url> {
        match self {
            Self::Media { resource } => resource.get_media_urls(),
            Self::Clip { edges } => edges.iter().flat_map(|x| x.node.get_media_urls()).collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ClipEdge {
    node: InstagramMedia,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "__typename")]
#[allow(clippy::enum_variant_names)]
enum InstagramMedia {
    XIGPolarisCarouselMedia {
        carousel_media: Vec<Self>,
    },
    #[serde(alias = "XDTMediaDict")]
    XIGPolarisVideoMedia(InstagramOrderedSimpleUrlList),
    XIGPolarisImageMedia {
        #[serde(alias = "image_versions2")]
        list: InstagramOrderedSimpleUrlList,
    },
    XDTClipsItemDict {
        media: Box<Self>,
    },
}
impl InstagramMedia {
    fn get_media_urls(&self) -> Vec<Url> {
        match self {
            Self::XIGPolarisCarouselMedia { carousel_media } => carousel_media
                .iter()
                .flat_map(Self::get_media_urls)
                .collect(),
            Self::XIGPolarisImageMedia { list } | Self::XIGPolarisVideoMedia(list) => {
                list.get_media_url().map_or_else(Vec::new, |x| vec![x])
            }
            Self::XDTClipsItemDict { media } => media.get_media_urls(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct InstagramOrderedSimpleUrlList {
    #[serde(alias = "candidates", alias = "video_versions")]
    urls: Vec<InstagramSimpleUrl>,
}
impl InstagramOrderedSimpleUrlList {
    fn get_media_url(&self) -> Option<Url> {
        self.urls.first().map(|x| x.url.clone())
    }
}

#[derive(derive_more::Debug, Serialize, Deserialize)]
struct InstagramSimpleUrl {
    #[debug("{:?}", url.as_str())]
    url: Url,
}

fn extract_info_from_html(html: &str) -> Option<InstagramStreamCache> {
    Html::parse_document(html)
        .select(&Selector::parse("script").expect("Invalid selector"))
        .filter_map(|x| {
            let text = x.text().collect::<String>();

            if !text.contains("RelayPrefetchedStreamCache") {
                return None;
            }

            Some(text)
        })
        .find_map(|script_data| {
            let val = serde_json::from_str::<serde_json::Value>(&script_data).ok()?;
            let val = find_stream_cache(val)?;

            serde_json::from_value::<InstagramStreamCache>(val).ok()
        })
}

fn find_stream_cache(val: serde_json::Value) -> Option<serde_json::Value> {
    match val {
        serde_json::Value::Object(obj) => {
            for val in obj.into_values() {
                if let Some(x) = find_stream_cache(val) {
                    return Some(x);
                }
            }
        }
        serde_json::Value::Array(mut arr) => {
            if arr.len() == 4
                && arr[0] == "RelayPrefetchedStreamCache"
                && let serde_json::Value::Array(mut val) = arr.remove(3)
                && val.len() >= 2
                && let serde_json::Value::Object(mut val) = val.remove(1)
                && let Some(serde_json::Value::Object(mut val)) = val.remove("__bbox")
                && let Some(serde_json::Value::Object(mut val)) = val.remove("result")
                && let Some(val) = val.remove("data")
            {
                return Some(val);
            }

            for item in arr {
                if let Some(x) = find_stream_cache(item) {
                    return Some(x);
                }
            }
        }
        _ => {}
    }

    None
}

fn extract_info_from_html_authed(html: &str, shortcode: &str) -> Option<Vec<Url>> {
    Html::parse_document(html)
        .select(&Selector::parse("script").expect("Invalid selector"))
        .filter_map(|x| {
            let text = x.text().collect::<String>();
            if !text.contains(shortcode) {
                return None;
            }
            Some(text)
        })
        .find_map(|script| {
            let val = serde_json::from_str::<serde_json::Value>(&script).ok()?;
            find_media_for_code(&val, shortcode)
        })
}

fn find_media_for_code(val: &serde_json::Value, code: &str) -> Option<Vec<Url>> {
    if let Some(c) = val.get("code").and_then(|v| v.as_str())
        && c == code
        && let Some(urls) = media_urls_from_container(val)
    {
        return Some(urls);
    }
    match val {
        serde_json::Value::Object(obj) => {
            for v in obj.values() {
                if let Some(found) = find_media_for_code(v, code) {
                    return Some(found);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                if let Some(found) = find_media_for_code(v, code) {
                    return Some(found);
                }
            }
        }
        _ => {}
    }
    None
}

fn media_urls_from_container(media: &serde_json::Value) -> Option<Vec<Url>> {
    let mut urls = Vec::new();
    if let Some(items) = media.get("carousel_media").and_then(|v| v.as_array()) {
        for item in items {
            if let Some(url) = best_media_url(item) {
                urls.push(url);
            }
        }
    }
    if urls.is_empty()
        && let Some(url) = best_media_url(media)
    {
        urls.push(url);
    }
    if urls.is_empty() { None } else { Some(urls) }
}

fn best_media_url(media: &serde_json::Value) -> Option<Url> {
    let url = media
        .get("video_versions")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.get("url"))
        .and_then(|u| u.as_str())
        .or_else(|| {
            media
                .get("image_versions2")
                .and_then(|v| v.get("candidates"))
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.get("url"))
                .and_then(|u| u.as_str())
        });
    url.and_then(|s| Url::parse(s).ok())
}
