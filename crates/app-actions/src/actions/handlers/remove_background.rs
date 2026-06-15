use std::{path::Path, string::ToString, time::Duration};

use app_helpers::{
    file_name::file_name_with_suffix,
    file_type::{infer_file_type, mime},
    futures::tryhard,
};
use futures::StreamExt;
use http::header;
use reqwest::{Body, multipart};
use serde::{Deserialize, Serialize};
use tokio::{fs::File, io::AsyncWriteExt};
use tokio_util::codec::{BytesCodec, FramedRead};
use tracing::{trace, warn};

use crate::{
    actions::{Action, ActionError, ActionRequest, ActionResult},
    common::request::Client,
    fixers::{FixRequest, Fixer, handlers::crop_image::CropImage},
};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RemoveBackground;

#[async_trait::async_trait]
#[typetag::serde]
impl Action for RemoveBackground {
    fn description(&self) -> &'static str {
        "Remove background from image"
    }

    async fn can_run_for(&self, req: &ActionRequest) -> bool {
        let file_mime = {
            let file_path = req.file_path.clone();
            tokio::task::spawn_blocking(move || infer_file_type(&file_path)).await
        };

        let file_mime = match file_mime {
            Ok(Ok(x)) => x,
            _ => return false,
        };

        matches!(file_mime.type_(), mime::IMAGE)
    }

    async fn run(&self, request: &ActionRequest) -> Result<ActionResult, ActionError> {
        trace!("Running remove background action");
        let output_file_path = request
            .output_dir
            .join(file_name_with_suffix(&request.file_path, "fg").with_extension("png"));

        trace!("Output file path: {output_file_path:?}");

        let client = Client::base().map_err(|e| {
            ActionError::FailedAction(format!("Failed to create client: {e:?}").into())
        })?;

        let remote_file_url = TempFileUpload::upload(&client, &request.file_path).await?;

        let bg_removed_url = tryhard::retry_fn(|| {
            let url = remote_file_url.file_url.clone();

            async move {
                trace!("Running remove background task");
                Client::base()
                    .map_err(|e| {
                        trace!("Failed to run remove background task: {e:?}");
                        ActionError::FailedAction(format!("Failed to create client: {e:?}").into())
                    })?
                    .post("https://www.birefnet.top/api/generate")
                    .header(header::CONTENT_TYPE, "application/json")
                    .json(&serde_json::json!({
                        "imageUrl": url,
                    }))
                    .send()
                    .await
                    .and_then(reqwest::Response::error_for_status)
                    .map_err(|e| {
                        trace!("Failed to run remove background task: {e:?}");
                        ActionError::FailedAction(
                            format!("Failed to run remove background task: {e:?}").into(),
                        )
                    })?
                    .json::<serde_json::Value>()
                    .await
                    .map(|x| x.as_str().unwrap_or_default().to_string())
                    .map_err(|e| {
                        trace!("Failed to run remove background task: {e:?}");
                        ActionError::FailedAction(
                            format!("Failed to parse remove background response: {e:?}").into(),
                        )
                    })
            }
        })
        .retries(5)
        .fixed_backoff(Duration::from_secs(1))
        .await;

        if let Err(e) = remote_file_url.delete_uploaded_file().await {
            warn!("Failed to delete uploaded file: {e:?}");
        }

        let bg_removed_url = bg_removed_url?;

        trace!(?bg_removed_url, "Removed background url");

        let mut file = File::create(&output_file_path).await.map_err(|e| {
            ActionError::FailedAction(format!("Failed to create output file: {e:?}").into())
        })?;

        trace!("Downloading file");
        let mut res = client
            .get(bg_removed_url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| {
                ActionError::FailedAction(format!("Failed to download file: {e:?}").into())
            })?
            .bytes_stream();

        while let Some(Ok(chunk)) = res.next().await {
            file.write_all(chunk.as_ref()).await.map_err(|e| {
                ActionError::FailedAction(format!("Failed to write to output file: {e:?}").into())
            })?;
        }
        trace!("File downloaded");
        file.flush().await.map_err(|e| {
            ActionError::FailedAction(format!("Failed to flush output file: {e:?}").into())
        })?;
        trace!("File flushed");

        let res = CropImage.run(&FixRequest::new(&output_file_path)).await;

        if let Ok(res) = res {
            if let Err(e) = tokio::fs::remove_file(&output_file_path).await {
                warn!(path = ?output_file_path, "Failed to remove file: {e:?}");
            }

            Ok(ActionResult::path(request, res.file_path))
        } else {
            Ok(ActionResult::path(request, output_file_path))
        }
    }
}

#[derive(Debug)]
struct TempFileUpload {
    file_url: String,
    token: Option<String>,
}
impl TempFileUpload {
    const SERVICE_URL: &'static str = "https://0x0.st";

    pub async fn upload(client: &reqwest::Client, file_path: &Path) -> Result<Self, ActionError> {
        let form = {
            let file = {
                let file = File::open(&file_path).await.map_err(|e| {
                    ActionError::FailedAction(format!("Failed to open file: {e:?}").into())
                })?;

                let metadata = file.metadata().await.map_err(|e| {
                    ActionError::FailedAction(format!("Failed to get file metadata: {e:?}").into())
                })?;

                let stream = FramedRead::new(file, BytesCodec::new());

                let stream = Body::wrap_stream(stream);

                let mut file = multipart::Part::stream_with_length(stream, metadata.len())
                    .file_name(
                        file_path
                            .file_name()
                            .map(|x| x.to_string_lossy().to_string())
                            .unwrap_or_default(),
                    );

                let file_mime = {
                    let path = file_path.to_path_buf();

                    tokio::task::spawn_blocking(move || infer_file_type(&path).ok())
                        .await
                        .ok()
                        .flatten()
                };

                if let Some(mime) = file_mime {
                    file = file.mime_str(mime.as_ref()).map_err(|e| {
                        ActionError::FailedAction(format!("Failed to set MIME type: {e:?}").into())
                    })?;
                }

                file
            };

            multipart::Form::new()
                .part("file", file)
                .part("expires", multipart::Part::text("1"))
                .part("secret", multipart::Part::text(""))
        };

        trace!(?form, "Built file upload form");

        trace!(url = ?Self::SERVICE_URL, "Uploading file to temp file upload service");
        let remote_upload_resp = client
            .post(Self::SERVICE_URL)
            .multipart(form)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| {
                ActionError::FailedAction(format!("Failed to run file upload task: {e:?}").into())
            })?;

        trace!(?remote_upload_resp, "File upload response");

        let remote_upload_token = remote_upload_resp
            .headers()
            .get("x-token")
            .and_then(|x| x.to_str().ok())
            .map(str::trim)
            .map(ToString::to_string);

        trace!(?remote_upload_token, "File upload token");

        let new = remote_upload_resp
            .text()
            .await
            .map(|x| x.trim().to_string())
            .map(|x| {
                let mut x = Self {
                    file_url: x,
                    token: None,
                };

                if let Some(token) = remote_upload_token {
                    x.token = Some(token);
                }

                x
            })
            .map_err(|e| {
                ActionError::FailedAction(
                    format!("Failed to parse file upload response: {e:?}").into(),
                )
            })?;

        trace!(?new, "File upload url");

        Ok(new)
    }

    async fn delete_uploaded_file(&self) -> Result<(), ActionError> {
        let token = match self.token.clone() {
            Some(x) => x,
            None => return Ok(()),
        };

        trace!(url = ?self.file_url, ?token, "Deleting remote file");

        let client = Client::base().map_err(|e| {
            ActionError::FailedAction(format!("Failed to create client: {e:?}").into())
        })?;

        let url = self.file_url.clone();

        let res = client
            .post(url)
            .multipart(
                multipart::Form::new()
                    .part("token", multipart::Part::text(token))
                    .part("delete", multipart::Part::text("")),
            )
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| {
                ActionError::FailedAction(format!("Failed to delete remote file: {e:?}").into())
            });

        trace!(?res, "Deleted remote file");

        res.map(|_| ())
    }
}
