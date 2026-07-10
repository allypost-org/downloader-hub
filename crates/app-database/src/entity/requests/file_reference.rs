use std::sync::Arc;

use serde::{Deserialize, Serialize};

#[derive(derive_more::Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileReference {
    Url(FileUrl),
    BlobTicket(BlobTicket),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobTicket {
    #[serde(rename = "t")]
    pub ticket: String,
    #[serde(rename = "f")]
    pub file_name: Arc<str>,
}

impl FileReference {
    pub fn url<T>(url: T) -> Self
    where
        T: Into<url::Url>,
    {
        Self::Url(url.into().into())
    }

    pub fn blob_ticket<T, F>(ticket: T, file_name: F) -> Self
    where
        T: Into<String>,
        F: Into<String>,
    {
        Self::BlobTicket(BlobTicket {
            ticket: ticket.into(),
            file_name: Arc::from(file_name.into()),
        })
    }
}

#[derive(derive_more::Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileUrl {
    #[debug("{:?}", url.as_str())]
    #[serde(rename = "u")]
    pub url: url::Url,

    #[serde(default, rename = "m")]
    pub method: Arc<str>,

    #[serde(default, rename = "h")]
    pub headers: Vec<(String, String)>,

    #[serde(default, with = "crate::helpers::serde::size::option", rename = "fsm")]
    pub max_filesize: Option<size::Size>,
}

impl FileUrl {
    #[must_use]
    pub fn new(url: url::Url) -> Self {
        Self {
            url,
            method: Arc::from("GET"),
            headers: Vec::new(),
            max_filesize: None,
        }
    }

    #[must_use]
    pub fn with_method<T>(mut self, method: T) -> Self
    where
        T: Into<Arc<str>>,
    {
        self.method = method.into();
        self
    }

    #[must_use]
    pub fn with_header<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.headers.push((key.into(), value.into()));
        self
    }

    #[must_use]
    pub fn with_headers<T, K, V>(mut self, headers: T) -> Self
    where
        T: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        self.headers = headers
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();

        self
    }

    #[must_use]
    pub const fn with_max_filesize(mut self, max_filesize: Option<size::Size>) -> Self {
        self.max_filesize = max_filesize;
        self
    }
}

impl From<url::Url> for FileUrl {
    fn from(value: url::Url) -> Self {
        Self::new(value)
    }
}
