use std::{result::Result, sync::LazyLock, time::Duration};

use app_requests::Client;
use http::{StatusCode, header};
use regex::Regex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use tracing::{debug, trace};
use url::Url;

use super::{ExtractInfoRequest, Extractor};
use crate::{config::ActionsConfig, extractors::ExtractedInfo};

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

    async fn extract_info(&self, request: &ExtractInfoRequest) -> Result<ExtractedInfo, String> {
        for i in 0_u32..3 {
            let Some(media_urls) = get_media_urls(request.url.as_str()).await? else {
                let delay = Duration::from_secs(2_u64.pow(i));
                debug!(
                    ?i,
                    ?delay,
                    "Failed to get media urls from post, retrying after delay",
                );
                tokio::time::sleep(delay).await;
                continue;
            };

            return Ok(ExtractedInfo::from_urls(request, media_urls));
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

#[tracing::instrument(skip_all, fields(url = %url))]
async fn get_media_urls(url: &str) -> Result<Option<Vec<Url>>, String> {
    trace!("Fetching instagram media URLs from post");

    let client = Client::sneaky().map_err(|e| format!("Failed to create client: {e:?}"))?;

    let resp = client
        .get(url)
        .header(
            header::USER_AGENT,
            ActionsConfig::request().user_agent.as_str(),
        )
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
