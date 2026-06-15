use std::{collections::HashSet, fmt::Write};

use app_helpers::file_type::{infer_file_type, mime};
use reqwest::{Body, multipart};
use serde::{Deserialize, Serialize};
use tokio::fs::File;
use tokio_util::codec::{BytesCodec, FramedRead};
use tracing::trace;

use crate::{
    actions::{Action, ActionError, ActionRequest, ActionResult},
    common::request::Client,
    config::ActionsConfig,
};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct OcrImage;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct OcrImageOptions {
    pub engine: Option<String>,
    #[serde(default)]
    pub list_engines: bool,
}

#[async_trait::async_trait]
#[typetag::serde]
impl Action for OcrImage {
    fn description(&self) -> &'static str {
        "Run OCR on an image. Depends on external service so may be randomly down."
    }

    async fn can_run(&self) -> bool {
        ActionsConfig::endpoints().ocr_api_base_url.is_some()
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
            && matches!(file_mime.subtype().as_str(), "png" | "jpeg" | "webp")
    }

    #[allow(clippy::too_many_lines)]
    async fn run(&self, request: &ActionRequest) -> Result<ActionResult, ActionError> {
        #[derive(Debug, Deserialize)]
        struct OcrEndpoint {
            available_handlers: Vec<String>,
        }

        #[derive(Debug, Deserialize)]
        struct OcrResponse {
            data: Vec<OcrResponseEntry>,
        }
        #[derive(Debug, Deserialize)]
        struct OcrResponseEntry {
            text: String,
        }

        let opts = request.options::<OcrImageOptions>().unwrap_or_default();

        trace!(?opts, "Running OCR action");

        let client = Client::base().map_err(|e| {
            ActionError::FailedAction(format!("Failed to create client: {e:?}").into())
        })?;

        if opts.list_engines {
            let url = ActionsConfig::endpoints()
                .ocr_api_url("endpoints")
                .ok_or_else(|| ActionError::FailedAction("OCR API URL not set".into()))?;

            trace!(url = ?url, "Listing OCR engines");

            let res = client
                .get(url)
                .send()
                .await
                .map_err(|e| {
                    ActionError::FailedAction(format!("Failed to list OCR engines: {e:?}").into())
                })?
                .json::<Vec<OcrEndpoint>>()
                .await
                .map_err(|e| {
                    ActionError::FailedAction(format!("Failed to parse OCR engines: {e:?}").into())
                })?;

            let handlers = res
                .into_iter()
                .flat_map(|x| x.available_handlers)
                .collect::<HashSet<_>>();

            let mut text = "Available OCR engines:\n".to_string();
            for handler in handlers {
                if let Err(e) = writeln!(text, "  - {}", handler) {
                    tracing::error!("Failed to write to text: {e:?}");
                }
            }
            text = text.trim().to_string();

            return Ok(ActionResult::text(request, text));
        }

        let engine = match opts.engine {
            Some(engine) => engine,
            None => {
                return Err(ActionError::FailedAction(
                    format!(
                        "No OCR engine specified. Please specify one using the engine param (eg. \
                         <code>/act {name} engine=ocrs</code>). You can see the available engines \
                         using <code>/act {name} list-engines</code>.",
                        name = self.name(),
                    )
                    .into(),
                ));
            }
        };

        let url = ActionsConfig::endpoints()
            .ocr_api_url(format!("ocr/{}", engine).as_str())
            .ok_or_else(|| ActionError::FailedAction("OCR API URL not set".into()))?;

        trace!(url = ?url, "Running OCR");

        let form = {
            let file = {
                let file = File::open(&request.file_path).await.map_err(|e| {
                    ActionError::FailedAction(format!("Failed to open file: {e:?}").into())
                })?;

                let stream = FramedRead::new(file, BytesCodec::new());

                let stream = Body::wrap_stream(stream);

                let mut file = multipart::Part::stream(stream).file_name(
                    request
                        .file_path
                        .file_name()
                        .map(|x| x.to_string_lossy().to_string())
                        .unwrap_or_default(),
                );

                let file_mime = {
                    let path = request.file_path.clone();

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

            multipart::Form::new().part("file", file)
        };

        trace!(?form, "Built OCR form");

        let res = client
            .post(url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| ActionError::FailedAction(format!("Failed to run OCR: {e:?}").into()))?
            .json::<OcrResponse>()
            .await
            .map_err(|e| {
                ActionError::FailedAction(format!("Failed to parse OCR response: {e:?}").into())
            })?;

        let text = res
            .data
            .into_iter()
            .map(|x| x.text)
            .collect::<Vec<_>>()
            .join("\n");

        return Ok(ActionResult::text(request, text));
    }
}
