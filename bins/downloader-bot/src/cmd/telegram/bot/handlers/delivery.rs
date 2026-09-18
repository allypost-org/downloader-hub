use std::{path::PathBuf, sync::Arc};

use app_helpers::temp_file::TempFile;
use teloxide::{
    payloads::SendMediaGroupSetters, prelude::Requester, requests::Request, types::ReplyParameters,
};
use tracing::{trace, warn};

use crate::cmd::{
    _common::request_processor::PlatformDelivery,
    telegram::bot::helpers::{
        file_group::files_to_input_media_groups, retried::try_send_to_retrying,
        status_message::StatusMessage,
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
        self.bot()
            .owner_id()
            .is_some_and(|owner_id| self.chat_id().as_user().is_some_and(|x| x == owner_id))
    }

    async fn copy_files_to_owner_dir(
        &self,
        files: &[(TempFile, Option<PathBuf>)],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        copy_files_to_save_dir(files, self.bot().owner_download_dir()).await
    }

    async fn send_batches(
        &self,
        files: &[(TempFile, Option<PathBuf>)],
    ) -> Vec<(Option<PathBuf>, String)> {
        let (media_groups, failed_files) = files_to_input_media_groups(
            Arc::clone(&self.bot().dependency_paths),
            files.iter().map(|(x, _)| x),
            self.max_filesize(),
        )
        .await;

        let replying_to_id = self.msg_replying_to_id();
        let chat_id = self.chat_id();
        let tg = self.bot_arc();
        let mut failures: Vec<(Option<PathBuf>, String)> = failed_files
            .into_iter()
            .map(|(path, err)| (Some(path), err))
            .collect();

        for (media_group, paths) in media_groups {
            let tg = Arc::clone(&tg);
            let res = try_send_to_retrying(
                chat_id,
                media_group,
                Box::new(move |chat_id, media_group| {
                    let tg = Arc::clone(&tg);
                    async move {
                        tg.send_media_group(chat_id, media_group)
                            .reply_parameters(
                                ReplyParameters::new(replying_to_id).allow_sending_without_reply(),
                            )
                            .send()
                            .await
                    }
                }),
            )
            .await;

            match res {
                Ok(_) => trace!("Media group sent"),
                Err(e) => {
                    warn!(?e, "Failed to send media group");
                    let error = e.to_string();
                    failures.extend(paths.into_iter().map(|path| (Some(path), error.clone())));
                }
            }
        }

        failures
    }
}

#[tracing::instrument(skip_all)]
async fn copy_files_to_save_dir(
    fixed_file_paths: &[(TempFile, Option<PathBuf>)],
    download_dir: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let download_dir = match download_dir {
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

pub use crate::cmd::_common::request_processor::{supervisor, watch_and_process};

pub async fn start_request_task(
    rpc: Arc<crate::peering::rpc::RpcClient>,
    request_id: Arc<str>,
    status_message: StatusMessage,
    is_recovery: bool,
) {
    supervisor()
        .start(
            request_id.clone(),
            watch_and_process(rpc, request_id, status_message, is_recovery),
        )
        .await;
}
