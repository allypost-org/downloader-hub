use std::sync::LazyLock;

use app_requests::Client;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::{debug, trace};
use url::Url;

use super::{ExtractInfoRequest, ExtractedInfo, Extractor, twitter::Twitter};
use crate::{
    downloaders::handlers::{generic::Generic, yt_dlp::YtDlp},
    extractors::ExtractedUrlInfo,
};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Bsky;

#[async_trait::async_trait]
#[typetag::serde]
impl Extractor for Bsky {
    fn description(&self) -> &'static str {
        "Downloads images and videos from BlueSky and screenshots the post itself."
    }

    async fn can_handle(&self, request: &ExtractInfoRequest) -> bool {
        Self::is_post_url(&request.url)
    }

    async fn extract_info(&self, request: &ExtractInfoRequest) -> Result<ExtractedInfo, String> {
        let mut urls = match Self::get_bsky_media_urls(&request.url).await {
            Ok(urls) => urls,
            Err(e) => {
                return Err(format!("Failed to get bsky media urls: {e}"));
            }
        };

        urls.push(Twitter.screenshot_tweet_url_info(&request.url));

        Ok(ExtractedInfo::from_urls(request, urls))
    }
}

impl Bsky {
    #[tracing::instrument(skip(post_url), fields(post_url = %post_url.as_str()))]
    pub async fn get_bsky_media_urls(post_url: &Url) -> Result<Vec<ExtractedUrlInfo>, String> {
        debug!("Getting bsky media urls for post url");

        let Some(parts) = BSKY_PATH_MATCHER.captures(post_url.path()) else {
            return Err("Invalid bsky post url".to_string());
        };

        let Some(post_id) = parts.name("postId").map(|x| x.as_str()) else {
            return Err("Invalid bsky post url. No post id".to_string());
        };

        let Some(username) = parts.name("username").map(|x| x.as_str()) else {
            return Err("Invalid bsky post url. No username".to_string());
        };

        trace!(?username, ?post_id, "Got bsky post id and user");

        let atproto_url = format!("at://{username}/app.bsky.feed.post/{post_id}");

        let api_url = {
            let mut url =
                Url::parse("https://public.api.bsky.app/xrpc/app.bsky.feed.getPostThread")
                    .expect("Invalid URL");

            url.query_pairs_mut()
                .extend_pairs([("uri", atproto_url.as_str())]);

            url
        };

        trace!(?api_url, "Got bsky api url");

        let resp = Client::base()?
            .get(api_url)
            .send()
            .await
            .map_err(|e| format!("Failed to get bsky media urls for post url. Error: {e}"))?
            .error_for_status()
            .map_err(|e| format!("Failed to get bsky media urls for post url. Error: {e}"))?
            .json::<GetPostThreadResponse>()
            .await
            .map_err(|e| format!("Failed to parse bsky media urls for post url. Error: {e}"))?;

        trace!(?resp, "Got response from bsky api");

        let media = resp.get_media();

        trace!(?media, "Got media from post");

        Ok(media)
    }
}

static BSKY_PATH_MATCHER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\/profile\/(?<username>[^/]+)\/post\/(?<postId>[a-zA-Z0-9]+)")
        .expect("Failed to compile regex")
});

impl Bsky {
    #[must_use]
    pub fn is_post_url(url: &Url) -> bool {
        let Some(domain) = url.domain() else {
            return false;
        };

        if domain != "bsky.app" {
            return false;
        }

        let path = url.path();

        BSKY_PATH_MATCHER.is_match(path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GetPostThreadResponse {
    thread: GetPostThreadResponseThread,
}
impl GetPostThreadResponse {
    pub fn get_media(&self) -> Vec<ExtractedUrlInfo> {
        match &self.thread {
            GetPostThreadResponseThread::ThreadViewPost { post } => post.get_media(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "$type")]
enum GetPostThreadResponseThread {
    #[serde(rename = "app.bsky.feed.defs#threadViewPost")]
    ThreadViewPost { post: PostView },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PostView {
    uri: String,
    cid: String,
    author: ProfileViewBasic,
    embed: Option<PostViewEmbed>,
    indexed_at: String,
}
impl PostView {
    pub fn get_media(&self) -> Vec<ExtractedUrlInfo> {
        match &self.embed {
            Some(PostViewEmbed::RecordWithMedia { media, .. } | PostViewEmbed::Media(media)) => {
                media.get_media()
            }
            _ => vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileViewBasic {
    did: String,
    handle: String,
    display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "$type")]
enum PostMedia {
    #[serde(rename = "app.bsky.embed.images#view", rename_all = "camelCase")]
    Images { images: Vec<ViewImage> },
    #[serde(rename = "app.bsky.embed.video#view", rename_all = "camelCase")]
    Video {
        cid: String,
        playlist: String,
        thumbnail: Option<String>,
        alt: Option<String>,
        aspect_ratio: Option<AspectRatio>,
    },
    #[serde(rename = "app.bsky.embed.external#view", rename_all = "camelCase")]
    External { external: ViewExternal },
}
impl PostMedia {
    pub fn get_media(&self) -> Vec<ExtractedUrlInfo> {
        match self {
            Self::Images { images, .. } => images
                .iter()
                .map(|x| {
                    ExtractedUrlInfo::new(&x.fullsize).with_preferred_downloader(Some(Generic))
                })
                .collect(),
            Self::Video { playlist, .. } => {
                vec![
                    ExtractedUrlInfo::new(playlist.as_str()).with_preferred_downloader(Some(YtDlp)),
                ]
            }
            Self::External { .. } => vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum PostViewEmbed {
    Media(PostMedia),
    Record {
        record: serde_json::Value,
    },
    RecordWithMedia {
        record: serde_json::Value,
        media: PostMedia,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AspectRatio {
    width: f64,
    height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ViewImage {
    thumb: String,
    fullsize: String,
    alt: String,
    aspect_ratio: Option<AspectRatio>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ViewExternal {
    uri: String,
    title: String,
    description: String,
    thumb: Option<String>,
}
