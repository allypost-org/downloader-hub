use std::{collections::HashMap, iter::Iterator};

use serde::Deserialize;
use tracing::{debug, trace};
use url::Url;

use super::{APHandler, HandleResult, node_info::NodeInfo};
use crate::{
    common::request::Client,
    extractors::{ExtractedUrlInfo, handlers::twitter::Twitter},
};

#[derive(Debug)]
pub struct MisskeyHandler;

#[async_trait::async_trait]
impl APHandler for MisskeyHandler {
    fn can_handle(&self, info: &NodeInfo, _url: &str) -> bool {
        matches!(
            info.software.name.to_lowercase().as_str(),
            "misskey" | "sharkey"
        )
    }

    #[tracing::instrument]
    async fn handle(&self, info: &NodeInfo, url: &str) -> Result<HandleResult, String> {
        let url = Url::parse(url).map_err(|e| e.to_string())?;

        let post_id = url
            .path_segments()
            .and_then(Iterator::last)
            .unwrap_or_default();

        trace!(?post_id, "Got post ID");

        let post_info = PostInfo::from_id(&url, post_id).await?;

        debug!(?post_info, "Got post info");

        let mut urls = post_info
            .media_urls()
            .into_iter()
            .map(|x| x.to_string().into())
            .collect::<Vec<ExtractedUrlInfo>>();

        urls.push(Twitter.screenshot_tweet_url_info(&url));

        Ok(HandleResult::Handled(urls))
    }
}

#[derive(Debug, Deserialize)]
struct PostInfo {
    files: Vec<PostFile>,
}
impl PostInfo {
    #[tracing::instrument(skip(url))]
    async fn from_id(url: &Url, id: &str) -> Result<Self, String> {
        let api_url = {
            let mut url = url.clone();

            url.set_path("/api/notes/show");
            url.query_pairs_mut().clear();

            url
        };
        trace!(?api_url, ?id, "Getting post info");

        let body: HashMap<_, _> = HashMap::from_iter([("noteId", id)]);

        Client::base()?
            .post(api_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Failed to get toot info: {:?}", e))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse toot info: {:?}", e))
    }

    fn media_urls(&self) -> Vec<Url> {
        self.files.iter().map(|x| x.url.clone()).collect()
    }
}

#[derive(Debug, Deserialize)]
struct PostFile {
    url: Url,
}
