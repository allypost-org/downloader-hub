use std::sync::LazyLock;

use node_info::{NodeInfo, get_node_info};
use serde::{Deserialize, Serialize};
use tracing::{debug, trace, warn};

use super::{ExtractInfoRequest, ExtractedInfo, Extractor};
use crate::extractors::ExtractedUrlInfo;

pub mod mastodon;
pub mod misskey;
pub mod node_info;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ActivityPub;

#[async_trait::async_trait]
#[typetag::serde]
impl Extractor for ActivityPub {
    fn description(&self) -> &'static str {
        "Gets image and video URLS from many ActivityPub instances. Also screenshots the post."
    }

    async fn can_handle(&self, request: &ExtractInfoRequest) -> bool {
        let url = request.url.to_string();
        let info = match get_node_info(&url).await {
            Ok(info) => info,
            Err(_) => {
                return false;
            }
        };

        HANDLERS
            .iter()
            .any(|handler| handler.can_handle(&info, &url))
    }

    async fn extract_info(&self, request: &ExtractInfoRequest) -> Result<ExtractedInfo, String> {
        let mut maybe_post_url = request.url.to_string();
        let mut seen_urls = vec![];

        'outer: while seen_urls.len() < 10 {
            debug!(url = ?maybe_post_url, "Handling URL");
            if seen_urls.contains(&maybe_post_url) {
                return Err(format!(
                    "URL loop detected ({}). Aborting.",
                    seen_urls.join(" -> ")
                ));
            }
            seen_urls.push(maybe_post_url.clone());

            let info = get_node_info(&maybe_post_url).await?;
            trace!(?info, "Got node info");

            for handler in HANDLERS.iter() {
                if !handler.can_handle(&info, &maybe_post_url) {
                    continue;
                }

                trace!(?handler, "Handling with handler");

                let result = match handler.handle(&info, &maybe_post_url).await {
                    Ok(result) => result,
                    Err(e) => {
                        warn!(?e, "Failed to handle URL");
                        continue;
                    }
                };

                trace!(?result, "Got result");

                match result {
                    HandleResult::Handled(urls) => {
                        trace!(?urls, "Got URLs");
                        return Ok(ExtractedInfo::from_urls(request, urls));
                    }
                    HandleResult::Delegated { url } => {
                        debug!(?url, "Delegating to another handler");
                        maybe_post_url = url;
                        continue 'outer;
                    }
                }
            }

            return Err(format!(
                "No handler found for {:?} on {} version {}",
                maybe_post_url, info.software.name, info.software.version
            ));
        }

        Err(format!("No handler found for {:?}", maybe_post_url))
    }
}

static HANDLERS: LazyLock<Vec<Box<dyn APHandler>>> = LazyLock::new(handlers);

#[async_trait::async_trait]
trait APHandler: std::fmt::Debug + Send + Sync {
    fn can_handle(&self, info: &NodeInfo, url: &str) -> bool;

    async fn handle(&self, info: &NodeInfo, url: &str) -> Result<HandleResult, String>;
}

#[derive(Debug)]
enum HandleResult {
    Handled(Vec<ExtractedUrlInfo>),
    Delegated { url: String },
}

fn handlers() -> Vec<Box<dyn APHandler>> {
    vec![
        Box::new(mastodon::MastodonHandler),
        Box::new(misskey::MisskeyHandler),
    ]
}
