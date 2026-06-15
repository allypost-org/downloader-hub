use app_actions::fixers::{FixRequest, fix_file_with};
use app_helpers::temp_dir::TempDir;
use tracing::{info, trace};

use super::{Handler, HandlerError, HandlerReturn};
use crate::queue::{
    common::file::FileId,
    task::{Task, TaskInfo},
};

#[derive(Debug)]
pub struct FixRequestHandler;

#[async_trait::async_trait]
impl Handler for FixRequestHandler {
    fn name(&self) -> &'static str {
        "fix-request"
    }

    fn can_handle(&self, task: &Task) -> bool {
        matches!(task.info(), TaskInfo::FixRequest { .. })
    }

    async fn handle(&self, task: &Task) -> Result<HandlerReturn, HandlerError> {
        trace!(?task, "Handling fix request");

        task.update_status_message("Processing the request...")
            .await;

        let TaskInfo::FixRequest {
            message: msg,
            fixers,
        } = task.info()
        else {
            return Err(HandlerError::Fatal("Invalid task info".to_string()));
        };

        trace!(?msg, "Got message from task");

        task.add_span_metadata(msg);

        info!(task_id = ?task.id(), "Handling fix request");

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

        let path_to_fix = file_id
            .download(temp_download_dir.path())
            .await
            .map_err(HandlerError::Fatal)?;

        task.update_status_message("Fixing file...").await;

        let fix_result = fix_file_with(fixers.clone(), FixRequest::new(path_to_fix)).await?;

        task.update_status_message("Uploading fixed file...").await;

        task.reply_with_files(vec![fix_result.file_path])
            .await
            .map_err(HandlerError::Fatal)?;

        Ok(HandlerReturn::default())
    }
}
