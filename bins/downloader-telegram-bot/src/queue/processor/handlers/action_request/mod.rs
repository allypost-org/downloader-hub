use app_actions::actions::{ActionRequest, ActionResultData};
use app_helpers::temp_dir::TempDir;
use tracing::{info, trace};

use super::{Handler, HandlerError, HandlerReturn};
use crate::queue::{Task, common::file::FileId, task::TaskInfo};

#[derive(Clone, Debug)]
pub struct ActionRequestHandler;

#[async_trait::async_trait]
impl Handler for ActionRequestHandler {
    fn name(&self) -> &'static str {
        "action-request"
    }

    fn can_handle(&self, task: &Task) -> bool {
        matches!(task.info(), TaskInfo::ActionRequest { .. })
    }

    async fn handle(&self, task: &Task) -> Result<HandlerReturn, HandlerError> {
        trace!(?task, "Handling fix request");

        task.update_status_message("Processing the request...")
            .await;

        let TaskInfo::ActionRequest {
            message: msg,
            action,
            options,
        } = task.info()
        else {
            return Err(HandlerError::Fatal("Invalid task info".to_string()));
        };

        trace!(?msg, "Got message from task");

        task.add_span_metadata(msg);

        info!(task_id = ?task.id(), "Handling action request");

        let Some(in_reply_to) = msg.reply_to_message() else {
            task.update_status_message("This needs to be a reply to a message containing media")
                .await;
            return Ok(HandlerReturn::default().cleanup_status_message(false));
        };

        trace!(?in_reply_to, "Got reply from message");

        let Some(file_id) = FileId::from_message(in_reply_to) else {
            task.update_status_message("This needs to be a reply to a message containing media")
                .await;
            return Ok(HandlerReturn::default().cleanup_status_message(false));
        };

        trace!(?file_id, "Got file id from message");

        let temp_download_dir = TempDir::in_tmp_with_prefix(format!(
            "downloader-hub.telegram-download.{}.",
            task.id()
        ))?;

        task.update_status_message("Downloading file...").await;

        let path_to_act_on = file_id
            .download(temp_download_dir.path())
            .await
            .map_err(HandlerError::Fatal)?;

        task.update_status_message("Running action on file...")
            .await;

        let req = ActionRequest::in_same_dir(path_to_act_on)
            .ok_or_else(|| HandlerError::Fatal("Failed to get action request".to_string()))?
            .with_options(options.clone());

        trace!(?req, "Got action request");

        if !action.can_run_for(&req).await {
            trace!("Action can't be run for this request");
            task.update_status_message("Action can't be run for this file")
                .await;
            return Ok(HandlerReturn::default().cleanup_status_message(false));
        }

        let action_result = action.run(&req).await?;

        trace!(?action_result, "Got action result");

        match action_result.data {
            ActionResultData::Paths(paths) => {
                if paths.is_empty() {
                    task.update_status_message("Action didn't produce any files")
                        .await;
                    return Ok(HandlerReturn::default().cleanup_status_message(false));
                }

                task.update_status_message("Uploading fixed file...").await;

                task.reply_with_files(paths)
                    .await
                    .map_err(HandlerError::Fatal)?;
            }
            ActionResultData::Text(text) => {
                if text.is_empty() {
                    task.update_status_message("Action didn't produce any text")
                        .await;
                    return Ok(HandlerReturn::default().cleanup_status_message(false));
                }

                task.send_additional_status_message(&text).await;
            }
        }

        Ok(HandlerReturn::default())
    }
}
