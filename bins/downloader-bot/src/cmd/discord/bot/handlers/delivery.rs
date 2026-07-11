use std::{path::PathBuf, sync::Arc};

use app_helpers::temp_file::TempFile;
use serenity::all::CreateMessage;
use tracing::{trace, warn};

use crate::cmd::{
    _common::request_processor::PlatformDelivery,
    discord::bot::{
        discord_bot::DiscordBot,
        helpers::{file_group::send_attachment_groups, status_message::StatusMessage},
    },
};

impl PlatformDelivery for StatusMessage {
    async fn update_status_message(&mut self, text: &str) {
        Self::update_message(self, text).await;
    }

    async fn send_supplemental_message(&self, text: &str) {
        self.send_additional_message(text).await;
    }

    async fn delete_status_message(&self) {
        self.delete_message().await;
    }

    fn is_owner_request(&self) -> bool {
        DiscordBot::owner_id().is_some_and(|owner_id| self.author_id() == owner_id)
    }

    async fn copy_files_to_owner_dir(
        &self,
        files: &[(TempFile, Option<PathBuf>)],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        copy_files_to_save_dir(files).await
    }

    async fn send_batches(
        &self,
        files: &[(TempFile, Option<PathBuf>)],
    ) -> Vec<(Option<PathBuf>, String)> {
        let max_bytes = u64::try_from(DiscordBot::max_payload_size().bytes())
            .expect("max payload size must not be negative");

        let reference = self.original_message_reference();
        let channel_id = self.channel_id();

        send_attachment_groups(
            files.iter().map(|(f, n)| (f, n.as_ref())),
            max_bytes,
            move |group: Vec<serenity::all::CreateAttachment>| {
                let reference = reference.clone();
                trace!(group_len = group.len(), "Uploading attachment group");
                let mut builder = CreateMessage::new().reference_message(reference);
                for att in group {
                    builder = builder.add_file(att);
                }
                async move {
                    match channel_id.send_message(DiscordBot::bot(), builder).await {
                        Ok(_) => {
                            trace!("Attachment group sent");
                            Ok(())
                        }
                        Err(e) => {
                            warn!(?e, "Failed to send attachment group");
                            Err(format!("failed to upload to Discord: {e}"))
                        }
                    }
                }
            },
        )
        .await
        .into_iter()
        .map(|(path, err)| (Some(path), err))
        .collect()
    }
}

#[tracing::instrument(skip_all)]
async fn copy_files_to_save_dir(
    fixed_file_paths: &[(TempFile, Option<PathBuf>)],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let download_dir = match DiscordBot::owner_download_dir() {
        Some(x) => x,
        None => return Ok(()),
    };

    for (file, suggested_name) in fixed_file_paths {
        let file: &std::path::Path = file.as_ref();
        let file_name = suggested_name
            .as_ref()
            .and_then(|n: &PathBuf| {
                let p: &std::path::Path = n.as_ref();
                p.file_name().map(std::borrow::ToOwned::to_owned)
            })
            .or_else(|| file.file_name().map(std::borrow::ToOwned::to_owned));

        let Some(file_name) = file_name else {
            continue;
        };
        let dest = download_dir.join(file_name);

        trace!(?file, ?dest, "Copying file to download directory");
        tokio::fs::copy(&file, &dest).await?;
        trace!(?file, ?dest, "Copied file to download directory");
    }

    Ok(())
}

/// Re-export so the message handler / startup path can ask the keyed
/// supervisor to start a request task with the Discord status message.
pub use crate::cmd::_common::request_processor::{supervisor, watch_and_process};

/// Kick off a supervised per-request task for Discord.
pub async fn start_request_task(
    request_id: Arc<str>,
    status_message: StatusMessage,
    is_recovery: bool,
) {
    supervisor()
        .start(
            request_id.clone(),
            watch_and_process(request_id, status_message, is_recovery),
        )
        .await;
}
