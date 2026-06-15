use std::time::Duration;

pub use reqwest::{Client as RequestClient, ClientBuilder as RequestClientBuilder, RequestBuilder};

use super::url::UrlWithMeta;

const DEFAULT_TIMEOUT_SECS: u64 = 30;

pub struct Client;

impl Client {
    pub fn base() -> Result<RequestClient, String> {
        Self::builder()
            .build()
            .map_err(|e| format!("Failed to create client: {:?}", e))
    }

    pub fn request_from_url(url: &UrlWithMeta) -> Result<RequestBuilder, String> {
        let mut builder = Self::base()?.request(url.method().clone(), url.url().as_str());

        for (k, v) in url.headers() {
            builder = builder.header(k, v);
        }

        Ok(builder)
    }

    pub fn builder() -> RequestClientBuilder {
        RequestClient::builder().timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
    }
}
