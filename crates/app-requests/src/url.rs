use http::{HeaderMap, HeaderValue, Method, header::IntoHeaderName};
use serde::{Deserialize, Serialize};
use url::Url;

pub type UrlHeaders = HeaderMap;

#[derive(derive_more::Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UrlWithMeta {
    #[debug("{:?}", url.as_str())]
    url: Url,
    #[serde(with = "http_serde::header_map", default)]
    headers: UrlHeaders,
    #[serde(with = "http_serde::method", default = "default_get")]
    method: Method,
}
impl UrlWithMeta {
    #[must_use]
    pub fn from_url_str(url: &str) -> Self {
        let url = Url::parse(url).unwrap_or_else(|_| panic!("Failed to parse URL: {:?}", url));

        Self::from_url(url)
    }

    #[must_use]
    pub fn from_url(url: Url) -> Self {
        Self {
            url,
            headers: UrlHeaders::default(),
            method: Method::GET,
        }
    }

    #[must_use]
    pub fn with_headers(mut self, headers: UrlHeaders) -> Self {
        self.headers = headers;
        self
    }

    #[must_use]
    #[allow(clippy::needless_pass_by_value)]
    pub fn with_header<K, V>(mut self, key: K, value: V) -> Self
    where
        K: IntoHeaderName,
        V: ToString,
    {
        let value = value.to_string();
        if let Ok(value) = HeaderValue::from_str(&value) {
            self.headers.append(key, value);
        }
        self
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
    pub const fn url(&self) -> &Url {
        &self.url
    }

    #[must_use]
    pub const fn headers(&self) -> &UrlHeaders {
        &self.headers
    }

    #[must_use]
    pub const fn method(&self) -> &Method {
        &self.method
    }
}

impl From<&str> for UrlWithMeta {
    fn from(url: &str) -> Self {
        Self::from_url_str(url)
    }
}
impl From<String> for UrlWithMeta {
    fn from(url: String) -> Self {
        Self::from_url_str(&url)
    }
}
impl From<&String> for UrlWithMeta {
    fn from(url: &String) -> Self {
        Self::from_url_str(url)
    }
}
impl From<Url> for UrlWithMeta {
    fn from(url: Url) -> Self {
        Self::from_url(url)
    }
}

impl PartialOrd for UrlWithMeta {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.url.partial_cmp(&other.url)
    }
}

const fn default_get() -> Method {
    Method::GET
}
