use std::sync::LazyLock;

use regex::Regex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;

use super::{ExtractInfoRequest, ExtractedInfo, Extractor, twitter::Twitter};
use crate::{downloaders::handlers::generic::Generic, extractors::ExtractedUrlInfo};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Tumblr;

#[async_trait::async_trait]
#[typetag::serde]
impl Extractor for Tumblr {
    fn description(&self) -> &'static str {
        "Downloads images and videos from Tumblr and screenshots the post itself."
    }

    async fn can_handle(&self, request: &ExtractInfoRequest) -> bool {
        Self::is_post_url(&request.url)
    }

    async fn extract_info(&self, request: &ExtractInfoRequest) -> Result<ExtractedInfo, String> {
        let mut info =
            ExtractedInfo::from_url(request, Twitter.screenshot_tweet_url_info(&request.url));

        if let Ok(post_media) = Self::fetch_post_media(&request.url).await {
            info = info.with_urls(post_media);
        }

        Ok(info)
    }
}

static DOMAIN_MATCH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:(?P<subdomain>[^\-][a-zA-Z0-9\-]{0,30}[^\-])\.)?tumblr\.com")
        .expect("Invalid regex")
});

impl Tumblr {
    pub fn is_post_url(url: &Url) -> bool {
        let Some(domain) = url.domain() else {
            return false;
        };

        DOMAIN_MATCH.is_match(domain)
    }
}

impl Tumblr {
    pub async fn fetch_post_media(url: &Url) -> Result<Vec<ExtractedUrlInfo>, String> {
        let html = app_requests::Client::base()
            .map_err(|e| format!("Failed to create client: {}", e))?
            .get(url.clone())
            .send()
            .await
            .map_err(|e| format!("Failed to get tumblr post: {}", e))?
            .error_for_status()
            .map_err(|e| format!("Got non-200 status code from tumblr: {}", e))?
            .text()
            .await
            .map_err(|e| format!("Failed to get html from tumblr: {}", e))?;

        tokio::task::spawn_blocking(move || Self::extract_post_media(&html))
            .await
            .map_err(|e| format!("Post media extraction crashed: {}", e))?
    }

    pub fn extract_post_media(html: &str) -> Result<Vec<ExtractedUrlInfo>, String> {
        let html = Html::parse_document(html);

        let el_post = html
            .select(&Selector::parse("article").expect("Invalid selector"))
            .next()
            .ok_or_else(|| "Failed to find post in html".to_string())?;

        let post_media = el_post
            .select(
                &Selector::parse(
                    "video > source, figure:not([aria-label=\"Avatar\"]) img:first-child",
                )
                .expect("Invalid selector"),
            )
            .filter_map(|el| {
                if let Some(src) = el.value().attr("src") {
                    return Some(ExtractedUrlInfo::new(src));
                }

                if let Some(srcset) = el.value().attr("srcset") {
                    let mut src_list = srcset
                        .split(',')
                        .map(str::trim)
                        .filter_map(|x| x.split_once(' '))
                        .map(|(url, resolution)| {
                            (
                                resolution[..resolution.len() - 1]
                                    .parse::<f64>()
                                    .unwrap_or(0.0),
                                url,
                            )
                        })
                        .collect::<Vec<_>>();

                    src_list.sort_by(|lt, gt| {
                        gt.0.partial_cmp(&lt.0).unwrap_or(std::cmp::Ordering::Equal)
                    });

                    if let Some(best_src) = src_list.first().map(|(_res, url)| url.to_string()) {
                        return Some(ExtractedUrlInfo::new(best_src));
                    }
                }

                None
            })
            .map(|x| {
                x.with_preferred_downloader(Some(Generic))
                    .with_header("Accept", "image/*, video/*, image/webp")
            })
            .collect::<Vec<_>>();

        Ok(post_media)
    }
}
