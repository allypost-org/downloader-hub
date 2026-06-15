use http::{HeaderMap, HeaderName, HeaderValue, Method, header};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    common::request::{Client, RequestBuilder},
    config::ActionsConfig,
};

#[derive(derive_more::Debug, Clone, Serialize, Deserialize)]
pub struct ExtractInfoRequest {
    #[debug("{:?}", url.as_str())]
    pub url: Url,
    #[serde(with = "http_serde::method", default = "default_get")]
    pub method: Method,
    #[serde(with = "http_serde::header_map", default)]
    pub headers: HeaderMap,
}

impl ExtractInfoRequest {
    #[must_use]
    pub fn new<T>(url: T) -> Self
    where
        T: Into<Url>,
    {
        Self {
            url: url.into(),
            method: Method::GET,
            headers: HeaderMap::default(),
        }
    }

    #[must_use]
    pub fn with_method<T>(mut self, method: T) -> Self
    where
        T: Into<Method>,
    {
        self.method = method.into();
        self
    }

    #[must_use]
    pub fn with_header<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<HeaderName>,
        V: Into<HeaderValue>,
    {
        self.headers.insert(key.into(), value.into());
        self
    }

    #[must_use]
    pub fn with_headers<T, K, V>(mut self, headers: T) -> Self
    where
        T: IntoIterator<Item = (K, V)>,
        K: Into<HeaderName>,
        V: Into<HeaderValue>,
    {
        self.headers = headers
            .into_iter()
            .fold(HeaderMap::new(), |mut map, (k, v)| {
                map.insert(k.into(), v.into());
                map
            });
        self
    }
}

impl ExtractInfoRequest {
    pub fn as_request_builder(&self) -> Result<RequestBuilder, String> {
        let mut builder = Client::base()?.request(
            self.method
                .as_str()
                .parse()
                .expect("Failed to parse method"),
            self.url.as_str(),
        );

        for (k, v) in &self.headers {
            builder = builder.header(k, v);
        }

        if !self.headers.contains_key(header::USER_AGENT) {
            builder = builder.header(
                header::USER_AGENT,
                ActionsConfig::request().user_agent.as_str(),
            );
        }

        if !self.headers.contains_key(header::ACCEPT) {
            builder = builder.header(header::ACCEPT, "*/*");
        }

        if !self.headers.contains_key(header::ACCEPT_LANGUAGE) {
            builder = builder.header(header::ACCEPT_LANGUAGE, "en-US,en;q=0.9");
        }

        Ok(builder)
    }
}

const fn default_get() -> Method {
    Method::GET
}

impl From<Url> for ExtractInfoRequest {
    fn from(url: Url) -> Self {
        Self::new(url)
    }
}

impl From<&Url> for ExtractInfoRequest {
    fn from(url: &Url) -> Self {
        url.clone().into()
    }
}

impl TryFrom<&str> for ExtractInfoRequest {
    type Error = url::ParseError;

    fn try_from(url: &str) -> Result<Self, Self::Error> {
        let parsed_url = Url::parse(url)?;

        Ok(parsed_url.into())
    }
}

impl TryFrom<String> for ExtractInfoRequest {
    type Error = url::ParseError;

    fn try_from(url: String) -> Result<Self, Self::Error> {
        url.as_str().try_into()
    }
}

impl TryFrom<&String> for ExtractInfoRequest {
    type Error = url::ParseError;

    fn try_from(url: &String) -> Result<Self, Self::Error> {
        url.as_str().try_into()
    }
}
