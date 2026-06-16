use std::process::Stdio;

use app_helpers::{
    file_name::file_name_with_suffix,
    file_type::{infer_file_type, mime},
};
use serde::{Deserialize, Serialize};
use tracing::trace;

use crate::actions::{Action, ActionError, ActionRequest, ActionResult};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CompactMedia;

#[async_trait::async_trait]
#[typetag::serde]
impl Action for CompactMedia {
    fn description(&self) -> &'static str {
        "Compact audio/video files by reducing resolution and/or bitrates."
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

        matches!(file_mime.type_(), mime::VIDEO | mime::AUDIO)
    }

    async fn run(&self, request: &ActionRequest) -> Result<ActionResult, ActionError> {
        trace!("Running compact video action");
        let output_file_path = request
            .output_dir
            .join(file_name_with_suffix(&request.file_path, "c"));

        trace!("Output file path: {output_file_path:?}");

        let mut cmd = tokio::process::Command::new("ffmpeg");
        cmd.arg("-i")
            .arg(&request.file_path)
            .args(["-max_muxing_queue_size", "1024"])
            .args(["-c:v", "libx264"])
            .args(["-crf", "29"])
            .args(["-af", "channelmap=0"])
            .args(["-c:a", "aac"])
            .args(["-b:a", "192k"])
            .args(["-vf", "scale=-2:480"])
            .args(["-preset", "slow"])
            .args(["-movflags", "+faststart"])
            .args(["-map_metadata", "-1"])
            .arg(&output_file_path)
            .stdin(Stdio::null());

        trace!("Running command: {cmd:?}");

        let output = cmd.output().await.map_err(|e| {
            ActionError::FailedAction(format!("Failed to run ffmpeg: {e:?}").into())
        })?;

        trace!("Command output: {output:?}");

        if !output.status.success() {
            return Err(ActionError::FailedAction(
                format!("ffmpeg exited with error code {output:?}").into(),
            ));
        }

        Ok(ActionResult::path(request, output_file_path))
    }
}
