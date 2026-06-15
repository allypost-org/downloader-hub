use std::{str::FromStr, sync::LazyLock};

use accept_header::Accept;
use axum::{
    http::{HeaderMap, HeaderValue},
    response::{IntoResponse, Response},
};
use mime::Mime;
use reqwest::{StatusCode, header};
use serde::Serialize;

static APPLICATION_POSTCARD: LazyLock<Mime> =
    LazyLock::new(|| Mime::from_str("application/postcard").expect("Invalid MIME type"));
pub struct JsonOrAccept<T>(pub T, pub HeaderMap);

impl<T> IntoResponse for JsonOrAccept<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        let headers = self.1;
        let accept = headers
            .get_all("accept")
            .iter()
            .filter_map(|x| x.to_str().ok())
            .flat_map(|x| x.split(','))
            .find(|x| x == &"application/json" || x == &"application/postcard")
            .unwrap_or("application/json")
            .parse::<Accept>();

        let Ok(accept) = accept else {
            return (
                StatusCode::NOT_ACCEPTABLE,
                [(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"))],
            )
                .into_response();
        };

        let negotiated = accept
            .negotiate(&[mime::APPLICATION_JSON, APPLICATION_POSTCARD.clone()])
            .unwrap_or(mime::APPLICATION_JSON);

        if negotiated == *APPLICATION_POSTCARD {
            into_postcard_response(self.0)
        } else {
            into_json_response(self.0)
        }
    }
}

fn into_json_response<T>(value: T) -> Response
where
    T: Serialize,
{
    // Use a small initial capacity of 128 bytes like serde_json::to_vec
    // https://docs.rs/serde_json/1.0.82/src/serde_json/ser.rs.html#2189
    let mut buf = Vec::with_capacity(128);
    match serde_json::to_writer(&mut buf, &value) {
        Ok(()) => (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            )],
            buf,
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"))],
            err.to_string(),
        )
            .into_response(),
    }
}

fn into_postcard_response<T>(value: T) -> Response
where
    T: Serialize,
{
    match postcard::to_allocvec(&value) {
        Ok(v) => (
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/postcard"),
            )],
            v,
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, HeaderValue::from_static("text/plain"))],
            err.to_string(),
        )
            .into_response(),
    }
}
