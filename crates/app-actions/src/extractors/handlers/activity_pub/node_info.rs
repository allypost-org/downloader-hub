use http::header;
use serde::Deserialize;
use tracing::{debug, trace};
use url::Url;

use crate::common::request::Client;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct NodeInfo {
    pub software: NodeInfoSoftware,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct NodeInfoSoftware {
    pub name: String,
    pub version: String,
}

#[tracing::instrument]
pub async fn get_node_info(node_url: &str) -> Result<NodeInfo, String> {
    #[derive(Debug, Deserialize)]
    struct NodeInfoList {
        links: Vec<NodeInfoLink>,
    }

    #[derive(Debug, Deserialize)]
    struct NodeInfoLink {
        rel: String,
        href: Url,
    }

    let url = {
        let mut url = Url::parse(node_url).map_err(|e| format!("Failed to parse URL: {:?}", e))?;

        url.set_path("/.well-known/nodeinfo");

        url
    };

    debug!("Getting NodeInfo");

    let client = Client::base()?;
    let info_list_resp = client
        .get(url)
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| format!("Failed to get NodeInfo: {:?}", e))?
        .text()
        .await
        .map_err(|e| format!("Failed to get NodeInfo: {:?}", e))?;

    trace!(len = info_list_resp.len(), "Got info list response");

    let info_list: NodeInfoList = serde_json::from_str(&info_list_resp)
        .map_err(|e| format!("Failed to parse NodeInfo: {:?}", e))?;

    trace!(?info_list, "Got info list");

    let info_url = info_list
        .links
        .into_iter()
        .find(|x| {
            x.rel
                .starts_with("http://nodeinfo.diaspora.software/ns/schema/2.")
        })
        .map(|x| x.href)
        .ok_or_else(|| "No NodeInfo URL found".to_string())?;

    trace!(?info_url, "Got info URL");

    client
        .get(info_url.as_str())
        .header(header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}
