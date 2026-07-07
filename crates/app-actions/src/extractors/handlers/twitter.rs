use std::{string::ToString, sync::LazyLock};

use app_requests::Client;
use http::{HeaderMap, header};
use jiff::ToSpan;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{debug, trace};
use url::Url;

use super::{ExtractInfoRequest, ExtractedInfo, Extractor};
use crate::{
    config::ActionsConfig, downloaders::handlers::generic::Generic, extractors::ExtractedUrlInfo,
};

pub static URL_MATCH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^https?://(www\.)?(twitter|x)\.com/(?P<username>[^/]+)/status/(?P<status_id>[0-9]+)",
    )
    .expect("Invalid regex")
});

pub static MEDIA_URL_MATCH: LazyLock<Regex> = LazyLock::new(|| {
    // https://pbs.twimg.com/media/FqPFEWYWYBQ5iG3?format=png&name=small
    Regex::new(r"^https?://pbs\.twimg\.com/media/").expect("Invalid regex")
});

static DEFAULT_AUTHORIZATION: &str = "Bearer AAAAAAAAAAAAAAAAAAAAANRILgAAAAAAnNwIzUejRCOuH5E6I8xnZz4puTs%\
     3D1Zv7ttfk8LF81IUq16cHjhLTvJu4FA33AGWWjCpTnA";

static TWEET_INFO_ENDPOINT: &str =
    "https://x.com/i/api/graphql/sCU6ckfHY0CyJ4HFjPhjtg/TweetResultByRestId";

static GUEST_TOKEN_ENDPOINT: &str = "https://api.x.com/1.1/guest/activate.json";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Twitter;

#[async_trait::async_trait]
#[typetag::serde]
impl Extractor for Twitter {
    fn description(&self) -> &'static str {
        "Downloads images and videos from Twitter posts"
    }

    async fn can_handle(&self, request: &ExtractInfoRequest) -> bool {
        Self::is_post_url(request.url.as_str())
    }

    async fn extract_info(&self, request: &ExtractInfoRequest) -> Result<ExtractedInfo, String> {
        debug!("Downloading tweet");

        let tweet_info = match get_tweet_info_from_url(request.url.as_str())? {
            Some(x) => x,
            None => {
                let screenshot_url = self.screenshot_tweet_url_info(&request.url);
                return Ok(ExtractedInfo::from_url(request, screenshot_url));
            }
        };

        trace!(?tweet_info, "Got tweet info");

        let tweet_data = get_tweet_data(&tweet_info.status_id).await?;

        trace!(?tweet_data, "Got tweet data");

        let mut tweet_media = get_tweet_media_urls(&tweet_data)
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect::<Vec<ExtractedUrlInfo>>();

        trace!(?tweet_media, "Got tweet media");

        if tweet_info.username != "i" {
            let tweet_screenshot_url = self.screenshot_tweet_url_info(&request.url);

            trace!("Adding Tweet screenshot URL: {:?}", &tweet_screenshot_url);
            tweet_media.push(tweet_screenshot_url);
        }

        Ok(ExtractedInfo::from_urls(request, tweet_media))
    }
}

impl Twitter {
    #[must_use]
    pub fn screenshot_tweet_url(&self, url: &Url) -> Url {
        let mut endpoint = ActionsConfig::endpoints()
            .twitter_screenshot_base_url
            .clone();

        endpoint.set_path(url.as_str());

        endpoint
    }

    #[must_use]
    pub fn screenshot_tweet_url_info(&self, url: &Url) -> ExtractedUrlInfo {
        ExtractedUrlInfo::new(self.screenshot_tweet_url(url))
            .with_preferred_downloader(Some(Generic))
            .with_downloader_options(Generic::options().with_timeout(Some(60.seconds())))
    }

    pub fn is_post_url(url: &str) -> bool {
        URL_MATCH.is_match(url)
    }

    pub fn is_media_url(url: &str) -> bool {
        MEDIA_URL_MATCH.is_match(url)
    }
}

#[derive(Debug)]
#[allow(dead_code)]
struct TweetInfo {
    username: String,
    status_id: String,
}
#[tracing::instrument]
fn get_tweet_info_from_url(url: &str) -> Result<Option<TweetInfo>, String> {
    let caps = match URL_MATCH.captures(url) {
        Some(caps) => caps,
        None => {
            trace!("Not a tweet url");
            return Ok(None);
        }
    };

    trace!(?caps, "Tweet URL match");

    let username = caps
        .name("username")
        .ok_or_else(|| "Couldn't get username from URL match".to_string())?
        .as_str();

    let status_id = caps
        .name("status_id")
        .ok_or_else(|| "Couldn't get status id from URL match".to_string())?
        .as_str();

    Ok(Some(TweetInfo {
        username: username.to_string(),
        status_id: status_id.to_string(),
    }))
}

#[derive(Debug)]
struct TweetData(serde_json::Value);

#[tracing::instrument]
async fn get_tweet_data(tweet_id: &str) -> Result<TweetData, String> {
    let guest_auth = get_guest_auth().await?;

    let query_params = {
        let graphql_variables = json!({
            "tweetId": tweet_id,
            "withCommunity": false,
            "includePromotedContent": false,
            "withVoice": false,
        })
        .to_string();
        let graphql_features = json!({
            "creator_subscriptions_tweet_preview_api_enabled": true,
            "communities_web_enable_tweet_community_results_fetch": true,
            "c9s_tweet_anatomy_moderator_badge_enabled": true,
            "articles_preview_enabled": true,
            "responsive_web_edit_tweet_api_enabled": true,
            "graphql_is_translatable_rweb_tweet_is_translatable_enabled": true,
            "view_counts_everywhere_api_enabled": true,
            "longform_notetweets_consumption_enabled": true,
            "responsive_web_twitter_article_tweet_consumption_enabled": true,
            "tweet_awards_web_tipping_enabled": false,
            "creator_subscriptions_quote_tweet_preview_enabled": false,
            "freedom_of_speech_not_reach_fetch_enabled": true,
            "standardized_nudges_misinfo": true,
            "tweet_with_visibility_results_prefer_gql_limited_actions_policy_enabled": true,
            "rweb_video_timestamps_enabled": true,
            "longform_notetweets_rich_text_read_enabled": true,
            "longform_notetweets_inline_media_enabled": true,
            "rweb_tipjar_consumption_enabled": true,
            "responsive_web_graphql_exclude_directive_enabled": true,
            "verified_phone_label_enabled": false,
            "responsive_web_graphql_skip_user_profile_image_extensions_enabled": false,
            "responsive_web_graphql_timeline_navigation_enabled": true,
            "responsive_web_enhance_cards_enabled": false,
        })
        .to_string();
        let field_toggles = json!({
            "withArticleRichContentState": true,
            "withArticlePlainText": false,
            "withGrokAnalyze": false,
            "withDisallowedReplyControls": false,
        })
        .to_string();

        &[
            ("variables", graphql_variables),
            ("features", graphql_features),
            ("fieldToggles", field_toggles),
        ]
    };

    trace!(?query_params, "Tweet info query params");

    let url = {
        let mut url = Url::parse(TWEET_INFO_ENDPOINT).expect("Invalid URL");
        url.query_pairs_mut().extend_pairs(query_params.iter());

        url.to_string()
    };

    trace!(?url, "Tweet info URL");

    let resp = Client::base()?
        .get(url)
        .headers(guest_auth.get_headers())
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {:?}", e))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Failed to parse response: {:?}", e))?;

    trace!(?resp, "Got response");

    resp.as_object()
        .and_then(|x| x.get("data"))
        .and_then(|x| x.as_object())
        .and_then(|x| x.get("tweetResult"))
        .and_then(|x| x.as_object())
        .and_then(|x| x.get("result"))
        .map(|x| TweetData(x.clone()))
        .ok_or_else(|| {
            format!(
                "Failed to get tweet data from response: {:?}",
                resp.to_string()
            )
        })
}

#[derive(Debug)]
pub enum TweetMedia {
    Photo { url: String },
    Video { url: String },
}
impl TweetMedia {
    #[must_use]
    pub fn as_url(&self) -> &str {
        match self {
            Self::Photo { url } | Self::Video { url } => url,
        }
    }
}
impl std::fmt::Display for TweetMedia {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_url().fmt(f)
    }
}

impl From<TweetMedia> for ExtractedUrlInfo {
    fn from(val: TweetMedia) -> Self {
        Self::new(val.as_url()).with_preferred_downloader(match val {
            TweetMedia::Photo { .. } => Some(Generic),
            TweetMedia::Video { .. } => None,
        })
    }
}

#[tracing::instrument(skip(tweet_data))]
fn get_tweet_media_urls(tweet_data: &TweetData) -> Option<Vec<TweetMedia>> {
    #[derive(Debug, Clone, Deserialize)]
    #[allow(dead_code)]
    struct RespTweetMediaVideoVariant {
        bitrate: Option<i64>,
        content_type: String,
        url: String,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct RespTweetMediaVideo {
        variants: Vec<RespTweetMediaVideoVariant>,
    }

    #[derive(Debug, Clone, Deserialize)]
    #[serde(tag = "type")]
    enum RespTweetMedia {
        #[serde(rename = "photo")]
        Photo {
            #[serde(rename = "media_url_https")]
            url: String,
        },
        #[serde(rename = "video")]
        #[serde(alias = "animated_gif")]
        Video { video_info: RespTweetMediaVideo },
    }
    impl RespTweetMedia {
        fn into_media(self) -> Option<TweetMedia> {
            match self {
                Self::Photo { url } => Some(TweetMedia::Photo { url }),
                Self::Video { video_info } => {
                    let mut variants = video_info.variants;
                    variants.sort_by(|lt, gt| {
                        gt.bitrate
                            .unwrap_or_default()
                            .cmp(&lt.bitrate.unwrap_or_default())
                    });

                    variants
                        .first()
                        .map(|x| TweetMedia::Video { url: x.url.clone() })
                }
            }
        }
    }

    trace!(?tweet_data, "Getting tweet media from tweet data");

    let mut tweet_media = tweet_data
        .0
        .get("legacy")
        .and_then(|x| x.as_object())
        .and_then(|x| x.get("extended_entities"))
        .and_then(|x| x.as_object())
        .and_then(|x| x.get("media"))
        .and_then(|x| serde_json::from_value::<Vec<RespTweetMedia>>(x.clone()).ok())
        .map(|x| {
            x.into_iter()
                .filter_map(RespTweetMedia::into_media)
                .collect::<Vec<_>>()
        });

    trace!(?tweet_media, "Got tweet media");

    if let Some(tm) = &tweet_media
        && tm.is_empty()
    {
        trace!("No tweet media found");
        tweet_media = None;
    }

    tweet_media
}

#[derive(Debug)]
struct GuestAuth {
    guest_token: String,
    cookie: Option<String>,
}
impl GuestAuth {
    fn get_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            DEFAULT_AUTHORIZATION
                .try_into()
                .expect("Invalid header value"),
        );
        headers.insert(
            "x-guest-token",
            self.guest_token
                .as_str()
                .try_into()
                .expect("Invalid header value"),
        );

        if let Some(cookie) = &self.cookie {
            headers.insert(
                header::COOKIE,
                cookie.try_into().expect("Invalid header value"),
            );
        }

        headers
    }
}

#[tracing::instrument]
async fn get_guest_auth() -> Result<GuestAuth, String> {
    #[derive(Deserialize)]
    struct GetGuestIdResponse {
        guest_token: String,
    }

    debug!("Getting Twitter guest auth token");

    let resp = Client::base()?
        .post(GUEST_TOKEN_ENDPOINT)
        .header(header::AUTHORIZATION, DEFAULT_AUTHORIZATION)
        .send()
        .await
        .map_err(|e| format!("Failed to send request: {:?}", e))?;

    trace!(?resp, "Got response");

    let cookie = resp
        .headers()
        .get("set-cookie")
        .and_then(|x| x.to_str().ok())
        .and_then(|x| x.split(';').next())
        .map(ToString::to_string);

    let guest_token = resp
        .json::<GetGuestIdResponse>()
        .await
        .map_err(|e| format!("Failed to parse response: {:?}", e))
        .map(|x| x.guest_token)?;

    debug!(?guest_token, ?cookie, "Got guest auth token");

    Ok(GuestAuth {
        guest_token,
        cookie,
    })
}
