use std::{ffi::OsString, path::PathBuf, string::ToString};

use app_config::common::Size;
use app_helpers::id::time_id;
use app_requests::Client;
use http::header;
use jiff::{Span, tz::TimeZone};
use mime2ext::mime2ext;
use serde::{Deserialize, Serialize};
use tokio::{fs::File, io::AsyncWriteExt};
use tracing::{debug, info, trace};
use unicode_segmentation::UnicodeSegmentation;
use url::Url;

use super::{DownloadRequest, DownloadResult, Downloader, DownloaderError, DownloaderReturn};
use crate::{
    ActionCtx,
    downloaders::{DownloaderOptions, helpers::headers::content_disposition},
};

pub const MAX_FILENAME_LENGTH: usize = 120;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Generic;

#[async_trait::async_trait]
#[typetag::serde]
impl Downloader for Generic {
    fn description(&self) -> &'static str {
        "Just tries to download exactly what you give it. No fancy tricks."
    }

    async fn can_download(&self, req: &DownloadRequest) -> bool {
        matches!(req.url.url().scheme(), "http" | "https")
    }

    async fn download(&self, _ctx: &ActionCtx, request: &DownloadRequest) -> DownloaderReturn {
        match self.download_one(request).await {
            Ok(x) => Ok(x),
            Err(DownloaderError::Error(e)) if request.fallibility().can_fail() => {
                Err(DownloaderError::FallibleFailed(e))
            }
            Err(e) => Err(e),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct GenericDownloaderOptions {
    #[serde(default)]
    max_filesize: Option<Size>,

    #[serde(default)]
    timeout: Option<Span>,
}
impl GenericDownloaderOptions {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub const fn with_timeout(mut self, timeout: Option<Span>) -> Self {
        self.timeout = timeout;
        self
    }
}
impl From<GenericDownloaderOptions> for DownloaderOptions {
    fn from(val: GenericDownloaderOptions) -> Self {
        let val = serde_json::to_value(val)
            .ok()
            .and_then(|x| x.as_object().cloned())
            .expect("Failed to serialize options");

        val.into_iter().collect()
    }
}

impl Generic {
    #[must_use]
    pub fn options() -> GenericDownloaderOptions {
        GenericDownloaderOptions::default()
    }

    #[allow(clippy::too_many_lines)]
    pub async fn download_one(
        &self,
        request_info: &DownloadRequest,
    ) -> Result<DownloadResult, DownloaderError> {
        let url = &request_info.url;
        let options = request_info
            .downloader_options::<GenericDownloaderOptions>()
            .unwrap_or_default();

        debug!(?options, "Running with downloader options");

        info!(?url, dir = ?request_info.download_dir(), "Downloading with generic downloader");

        let mut res = Client::request_from_url(url)
            .map_err(DownloaderError::Error)?
            .headers(url.headers().clone());

        if let Some(timeout) = options.timeout {
            let relative = jiff::Timestamp::now().to_zoned(TimeZone::UTC);
            let timeout = timeout
                .to_duration(&relative)
                .expect("downloader timeout must resolve to a concrete duration");
            res = res.timeout(
                std::time::Duration::try_from(timeout.abs())
                    .expect("downloader timeout must fit in std::time::Duration"),
            );
        }

        let mut res = res
            .send()
            .await
            .map_err(|e| DownloaderError::Error(format!("Failed to send request: {e:?}")))?
            .error_for_status()
            .map_err(|e| DownloaderError::Error(format!("Failed to get response: {e:?}")))?;

        let max_filesize = options.max_filesize;
        if let (Some(limit), Some(content_length)) = (max_filesize, res.content_length())
            && content_length > limit.bytes().cast_unsigned()
        {
            return Err(DownloaderError::ExceedsMaxFilesize { limit });
        }

        let mime_type = res.headers().get(header::CONTENT_TYPE).map(|x| x.to_str());
        debug!(?mime_type, "Got mime type");
        let mime_type = match mime_type {
            Some(Ok(mime_type)) => mime_type,
            _ => "",
        };

        let extension =
            mime2ext(mime_type).map_or_else(|| "unknown".to_string(), |x| (*x).to_string());

        debug!(?extension, "Got extension");

        let id = time_id();
        let mut file_name = OsString::from(&id);

        let taken_filename_len = id.len() + 1 + extension.len();

        let req_file_name = res
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .and_then(|x| content_disposition::ContentDisposition::from_raw(x).ok())
            .and_then(|x| {
                debug!(?x, "Got content disposition");
                x.get_filename_ext()
                    .and_then(content_disposition::ExtendedValue::try_decode)
                    .or_else(|| x.get_filename().map(ToString::to_string))
                    .map(|x| {
                        let trunc_idx =
                            x.floor_char_boundary(MAX_FILENAME_LENGTH - 1 - taken_filename_len);
                        x[..trunc_idx].to_string()
                    })
            })
            .or_else(|| {
                let url = url.url();
                debug!(?url, "Using url as filename");
                url_to_filename(url, taken_filename_len).map(|x| x + ".bin")
            })
            .unwrap_or_else(|| "unknown.bin".to_string());

        trace!(?req_file_name, "Got file name from request");

        file_name.push(".");
        file_name.push(req_file_name);

        let file_path = request_info.download_dir().join(file_name);
        debug!(?file_path, "Writing to file");
        let mut out_file = File::create(&file_path)
            .await
            .map_err(|e| DownloaderError::Error(format!("Failed to create file: {e:?}")))?;

        let max_bytes = max_filesize.map_or(u64::MAX, |x| x.bytes().cast_unsigned());
        let mut total_bytes_read = 0_u64;
        while let Some(chunk) = res
            .chunk()
            .await
            .map_err(|e| DownloaderError::Error(format!("Failed to get chunk: {e:?}")))?
        {
            let chunk_len = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
            total_bytes_read = total_bytes_read.saturating_add(chunk_len);
            if total_bytes_read > max_bytes {
                drop(out_file);
                if let Err(e) = tokio::fs::remove_file(&file_path).await {
                    debug!(
                        ?e,
                        ?file_path,
                        "Failed to remove oversized partial download"
                    );
                }
                return Err(DownloaderError::ExceedsMaxFilesize {
                    limit: max_filesize.expect("finite max bytes require a configured limit"),
                });
            }

            out_file
                .write_all(&chunk)
                .await
                .map_err(|e| DownloaderError::Error(format!("Failed to write chunk: {e:?}")))?;
        }

        Ok(DownloadResult {
            request: request_info.clone(),
            path: file_path,
        })
    }
}

fn url_to_filename(url: &Url, taken_filename_len: usize) -> Option<String> {
    Some(url).map(|x| PathBuf::from(x.path())).and_then(|x| {
        let stem = x.file_stem()?;

        let trunc = stem
            .to_string_lossy()
            .graphemes(true)
            .filter(|x| !x.chars().all(char::is_control))
            .filter(|x| !x.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|']))
            .take(MAX_FILENAME_LENGTH - 1 - taken_filename_len)
            .collect::<String>();

        if trunc.is_empty() { None } else { Some(trunc) }
    })
}
