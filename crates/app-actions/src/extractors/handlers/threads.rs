use std::sync::LazyLock;

use app_helpers::tree_yielder::TreeYielder;
use app_requests::Client;
use http::{HeaderName, HeaderValue};
use regex::Regex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use tracing::trace;
use url::Url;

use crate::extractors::{ExtractInfoRequest, ExtractedInfo, Extractor};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Threads;

#[async_trait::async_trait]
#[typetag::serde]
impl Extractor for Threads {
    fn description(&self) -> &'static str {
        "Gets media from Threads posts"
    }

    async fn can_handle(&self, request: &ExtractInfoRequest) -> bool {
        static POST_REGEX: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"^/@(?<username>[^/]+)/post/(?<post_id>[a-zA-Z0-9_-]+)$")
                .expect("Failed to compile regex")
        });

        request
            .url
            .host_str()
            .is_some_and(|x| matches!(x, "threads.com" | "www.threads.com"))
            && POST_REGEX.is_match(request.url.path())
    }

    async fn extract_info(&self, request: &ExtractInfoRequest) -> Result<ExtractedInfo, String> {
        let url = {
            let mut url = request.url.clone();
            url.query_pairs_mut().clear();
            url
        };

        trace!(url = ?url.as_str(), "Getting threads post");

        let req = {
            let headers = [
                (
                    "User-Agent",
                    "Mozilla/5.0 (X11; Linux x86_64; rv:148.0) Gecko/20100101 Firefox/148.0",
                ),
                (
                    "Accept",
                    "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
                ),
                ("Accept-Language", "en-US,en;q=0.9,hr;q=0.8"),
                ("Accept-Encoding", "gzip, deflate, br"),
                ("DNT", "1"),
                ("Sec-GPC", "1"),
                ("Connection", "close"),
                ("Upgrade-Insecure-Requests", "1"),
                ("Sec-Fetch-Dest", "document"),
                ("Sec-Fetch-Mode", "navigate"),
                ("Sec-Fetch-Site", "none"),
                ("Sec-Fetch-User", "?1"),
                ("Priority", "u=0, i"),
                ("Pragma", "no-cache"),
                ("Cache-Control", "no-cache"),
            ]
            .into_iter()
            .map(|(k, v): (&str, &str)| {
                let k: HeaderName = k.parse().expect("Failed to parse header name");
                let v: HeaderValue = v.parse().expect("Failed to parse header value");
                (k, v)
            })
            .collect();

            Client::base()?.get(url).headers(headers)
        };

        trace!(?req, "Sending request to threads");

        let resp = req
            .send()
            .await
            .map_err(|e| format!("Failed to send request: {:?}", e))?;

        trace!(?resp, "Got response from threads");

        let resp = resp
            .error_for_status()
            .map_err(|e| format!("Got error for status: {:?}", e))?;

        let html = resp
            .text()
            .await
            .map_err(|e| format!("Failed to get text from response: {:?}", e))?;

        trace!(len = html.len(), "Got html from threads");

        let dom = Html::parse_document(&html);

        trace!("Parsed html from threads");

        let page_data_raw = dom
            .select(&Selector::parse("script").expect("Invalid selector"))
            .find_map(|x| {
                if x.value().attr("type")? != "application/json" {
                    return None;
                }

                let text = x.text().collect::<String>();

                if !text.contains("mp4") {
                    return None;
                }

                Some(text)
            })
            .ok_or_else(|| {
                "Failed to find script with type application/json and containing mp4".to_string()
            })?;

        trace!(len = page_data_raw.len(), "Got page data from threads");

        let page_data = serde_json::from_str::<serde_json::Value>(&page_data_raw)
            .map_err(|e| format!("Failed to deserialize page data: {:?}", e))?;

        trace!("Deserialized page data from threads");

        // Find all string values
        let yielder = TreeYielder::new(|v| {
            let Some(obj) = v.as_object() else {
                return false;
            };
            let Some(result) = obj.get("result") else {
                return false;
            };
            let Some(result) = result.as_object() else {
                return false;
            };

            result.contains_key("data")
        });

        let media_result = yielder
            .find_first(&page_data)
            .ok_or_else(|| "Failed to find media object inside page data".to_string())?;

        trace!("Found tentative media object in page data");

        let obj = media_result
            .as_object()
            .and_then(|x| {
                x.get("result")?
                    .as_object()?
                    .get("data")?
                    .as_object()?
                    .get("media")
            })
            .ok_or_else(|| "Failed to find media object inside page data".to_string())?;

        trace!("Found actual media object inside page data");

        let obj = serde_json::from_value::<InstagramPost>(obj.clone())
            .map_err(|e| format!("Failed to deserialize media object: {:?}", e))?;

        trace!(?obj, "Deserialized media object");

        let media_urls = obj
            .media_urls()
            .ok_or_else(|| "No media urls found in media object".to_string())?;

        trace!(?media_urls, "Found media urls in media object");

        Ok(ExtractedInfo::from_urls(request, media_urls))
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InstagramPost {
    #[serde(alias = "carousel_media")]
    pub carousel_media: Option<Vec<InstagramMedia>>,
    #[serde(alias = "image_versions2")]
    pub image: Option<InstagramImageV2>,
    #[serde(alias = "video_versions")]
    pub video: Option<InstagramVideo>,
}
impl InstagramPost {
    pub fn media_urls(&self) -> Option<Vec<Url>> {
        if let Some(video) = self.video.as_ref()
            && let Some(video_url) = video.get_media_url()
        {
            return Some(vec![video_url]);
        }

        if let Some(carousel_media) = self.carousel_media.as_ref() {
            return Some(
                carousel_media
                    .iter()
                    .filter_map(InstagramMedia::get_media_url)
                    .collect(),
            );
        }

        if let Some(image) = self.image.as_ref()
            && let Some(image_url) = image.get_media_url()
        {
            return Some(vec![image_url]);
        }

        None
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstagramVideo(Vec<InstagramVideoVersion>);
impl InstagramVideo {
    #[must_use]
    pub fn get_media_url(&self) -> Option<Url> {
        self.0.first().map(|x| x.url.clone())
    }
}

#[derive(derive_more::Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InstagramVideoVersion {
    pub height: Option<f64>,
    pub width: Option<f64>,
    #[serde(alias = "type")]
    pub ty: u32,
    #[debug("{:?}", url.as_str())]
    pub url: Url,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InstagramMedia {
    pub id: String,
    #[serde(alias = "image_versions2")]
    pub image: InstagramImageV2,
}
impl InstagramMedia {
    pub fn get_media_url(&self) -> Option<Url> {
        self.image.get_media_url()
    }
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct InstagramImageV2 {
    pub candidates: Vec<InstagramImageV2Candidate>,
}
impl InstagramImageV2 {
    pub fn get_media_url(&self) -> Option<Url> {
        self.candidates.first().map(|x| x.url.clone())
    }
}

#[derive(derive_more::Debug, Clone, serde::Deserialize, serde::Serialize)]
struct InstagramImageV2Candidate {
    pub height: f64,
    pub width: f64,
    #[debug("{:?}", url.as_str())]
    pub url: Url,
}
