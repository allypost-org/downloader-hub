use std::{str::FromStr, sync::LazyLock};

use accept_header::Accept;
use axum::http::HeaderMap;
use mime::Mime;

static APPLICATION_POSTCARD: LazyLock<Mime> =
    LazyLock::new(|| Mime::from_str("application/postcard").expect("Invalid MIME type"));

#[derive(Debug)]
pub enum Negotiated {
    Json,
    Postcard,
}

impl Negotiated {
    pub fn from_mime(mime: mime::Mime) -> Option<Self> {
        match mime {
            x if x == mime::APPLICATION_JSON => Some(Self::Json),
            x if x == *APPLICATION_POSTCARD => Some(Self::Postcard),
            _ => None,
        }
    }

    pub fn negotiate(headers: &HeaderMap) -> Option<Self> {
        let accept = headers
            .get_all("accept")
            .iter()
            .filter_map(|x| x.to_str().ok())
            .flat_map(|x| x.split(','))
            .map(str::parse::<Accept>)
            .find_map(std::result::Result::ok)?;

        accept
            .negotiate(&[mime::APPLICATION_JSON, APPLICATION_POSTCARD.clone()])
            .ok()
            .and_then(Self::from_mime)
    }
}
