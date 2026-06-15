use std::{
    fs::Metadata,
    ops::Sub,
    path::{Path, PathBuf},
};

use app_helpers::file_type::infer_file_type;
use axum::{
    body::Body,
    http::{
        HeaderMap, HeaderValue, Request, StatusCode,
        header::{self, IntoHeaderName},
    },
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use tower_http::services::ServeFile;
use tracing::warn;

pub struct RangeResponder {
    path: PathBuf,
    metadata: Option<Metadata>,
    additional_headers: HeaderMap,
}
impl RangeResponder {
    pub fn from_path<T: AsRef<Path>>(path: T) -> Self {
        Self {
            path: path.as_ref().into(),
            metadata: None,
            additional_headers: HeaderMap::new(),
        }
    }

    pub fn add_header<TName>(&mut self, name: TName, value: HeaderValue) -> &mut Self
    where
        TName: IntoHeaderName,
    {
        self.additional_headers.insert(name, value);
        self
    }

    fn additional_headers(&self) -> HeaderMap {
        let mut res = self.additional_headers.clone();

        {
            let path = self.path.as_ref();
            let content_type = infer_file_type(path);
            if let Ok(content_type) = content_type
                && let Ok(content_type) = content_type.essence_str().parse()
            {
                res.append(header::CONTENT_TYPE, content_type);
            }

            if cfg!(debug_assertions)
                && let Ok(path) = path.to_string_lossy().parse()
            {
                res.append("X-File-Path", path);
            }
        }

        if let Some(metadata) = self.metadata.as_ref() {
            if let Ok(mtime) = metadata.modified() {
                let time: DateTime<Utc> = mtime.into();

                if let Ok(time) = time.to_rfc2822().parse() {
                    res.append(header::LAST_MODIFIED, time);
                }
            }

            if let Ok(ctime) = metadata.created() {
                let time: DateTime<Utc> = ctime.into();

                if let Ok(time) = time.to_rfc2822().parse() {
                    res.append("X-Created-At", time);
                }
            }
        }

        res
    }

    async fn add_metadata(&mut self) -> tokio::io::Result<&Self> {
        if self.metadata.as_ref().is_some() {
            return Ok(self);
        }

        let file = tokio::fs::File::open(&self.path).await?;

        let metadata = file.metadata().await?;
        self.metadata = Some(metadata);

        Ok(self)
    }

    async fn try_add_metadata(&mut self) -> &Self {
        let _ = self.add_metadata().await;
        self
    }

    fn respond_to_cache_headers(&self, req_headers: &HeaderMap) -> Result<(), StatusCode> {
        if let Some(metadata) = self.metadata.as_ref() {
            let req_time = req_headers
                .get(header::IF_MODIFIED_SINCE)
                .and_then(|x| x.to_str().ok())
                .and_then(|x| DateTime::parse_from_rfc2822(x).ok())
                .map(|x| x.with_timezone(&Utc));

            if let Some(req_time) = req_time
                && let Ok(mtime) = metadata.modified()
            {
                let file_time: DateTime<Utc> = mtime.into();

                if file_time.sub(req_time) > chrono::Duration::milliseconds(10) {
                    return Err(StatusCode::NOT_MODIFIED);
                }
            }
        }

        let etag = self.additional_headers.get(header::ETAG);

        if let Some(etag) = etag {
            let if_none_match = req_headers
                .get(header::IF_NONE_MATCH)
                .and_then(|x| x.to_str().ok());

            if let Some(if_none_match) = if_none_match {
                if if_none_match == "*" {
                    return Err(StatusCode::NOT_MODIFIED);
                }

                let if_none_match = if_none_match.split(',').map(str::trim);

                for match_etag in if_none_match {
                    if etag == match_etag {
                        return Err(StatusCode::NOT_MODIFIED);
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn into_response(mut self, request_headers: HeaderMap) -> axum::response::Response {
        self.try_add_metadata().await;
        if let Err(e) = self.respond_to_cache_headers(&request_headers) {
            return e.into_response();
        }

        let mut request = Request::new(Body::empty());
        *request.headers_mut() = request_headers;

        let response = ServeFile::new(&self.path).try_call(request).await;
        let mut response = match response {
            Ok(response) => response,
            Err(e) => {
                warn!(?e, "Failed to serve file");

                return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
            }
        };

        let res_headers = response.headers_mut();

        let mut additional_headers = self.additional_headers();
        additional_headers.remove(header::CONTENT_LENGTH);
        for header_name in additional_headers.keys() {
            let header_values = additional_headers.get_all(header_name);
            for header_value in header_values {
                res_headers.insert(header_name, header_value.clone());
            }
        }

        response.into_response()
    }
}
