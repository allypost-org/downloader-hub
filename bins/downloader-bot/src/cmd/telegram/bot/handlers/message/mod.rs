use std::{
    fmt::Write,
    sync::{Arc, LazyLock, Mutex},
};

use app_helpers::temp_file::TempFile;
use app_peer_comms::{
    Message as PeerMessage,
    message::v1::{
        V1Message,
        central::{CentralMessage, create_result::CreateResult, work_request::WorkRequest},
        common::{
            file::{FileReference, FileUrl},
            request_info::RequestInfo,
        },
    },
};
use futures::future;
use teloxide::{
    prelude::*,
    types::{Message as TelegramMessage, MessageEntityKind, ReplyParameters},
};
use tokio::sync::Semaphore;
use tracing::{debug, info, trace, warn};
use url::Url;

use crate::{
    cmd::{
        _common::work_request::{WorkRequestGuard, WorkRequestLockMap},
        telegram::{
            bot::{
                TelegramBot,
                helpers::{
                    file_group::files_to_input_media_groups, file_id::FileId,
                    status_message::StatusMessage,
                },
            },
            common::downloadable::Downloadable,
        },
    },
    peering::rpc::{RpcClient, RpcResponse},
};

pub async fn handle_message(msg: &TelegramMessage) -> ResponseResult<()> {
    info!("Adding download request to queue");

    let mut status_message = StatusMessage::from_message(msg);

    let file_id = FileId::from_message(msg);
    let file_urls = {
        let mut urls = urls_in_message(msg);
        urls.sort();
        urls
    };

    if file_id.is_none() && file_urls.is_empty() {
        status_message
            .update_message("Message doesn't contain any file or URL")
            .await;

        return Ok(());
    }

    status_message.update_message("Processing message...").await;

    let mut added_some = false;
    for (i, file_url) in file_urls.into_iter().enumerate() {
        let mut url_status_message = status_message
            .send_sub_message(&format!("Processing URL: {}", file_url))
            .await
            .unwrap_or_else(|| status_message.clone());

        let resp = RpcClient::work_request_create(
            RequestInfo::DownloadAndFix({
                let file_url: FileUrl = file_url.into();

                FileReference::url(file_url.with_max_filesize(Some(
                    TelegramBot::max_payload_size().bytes().cast_unsigned(),
                )))
            }),
            url_status_message.to_metadata(),
            Some(format!("tg-{}-{}-{}", msg.chat.id, msg.id, i)),
        )
        .await;

        trace!(?resp, "Got RPC response");

        let resp = match resp {
            Ok(RpcResponse::Data(data)) => data,
            Ok(RpcResponse::Error(e)) => {
                url_status_message
                    .update_message(&format!("Failed to add URL to queue: {}", e))
                    .await;

                continue;
            }
            Err(e) => {
                url_status_message
                    .update_message(&format!("Failed to add URL to queue: {}", e))
                    .await;

                continue;
            }
        };

        let Some(PeerMessage::V1(V1Message::Central(CentralMessage::WorkRequestCreateResponse(
            result,
        )))) = resp
        else {
            url_status_message
                .update_message(
                    "Failed to add request to queue: Got unknown response. Please report this to \
                     the bot developer.",
                )
                .await;

            continue;
        };

        #[allow(irrefutable_let_patterns)]
        let CreateResult::Ok(result) = result else {
            url_status_message
                .update_message("Failed to add request to queue")
                .await;

            continue;
        };

        url_status_message
            .update_message(&format!(
                "Request added to queue with ID <code>{}</code>",
                result.id
            ))
            .await;

        added_some = true;
    }

    if !added_some {
        status_message
            .update_message("Failed to add any requests to queue")
            .await;

        return Ok(());
    }

    status_message.delete_message().await;

    Ok(())
}

#[tracing::instrument(name="process-work-request", skip_all, fields(request_id = ?work_request.request_id))]
#[allow(clippy::too_many_lines)]
pub async fn process_work_request(
    work_request: Arc<WorkRequest>,
    mut status_message: StatusMessage,
) -> ResponseResult<()> {
    static WORK_REQUESTS_PROCESSING_LOCKS: LazyLock<Arc<Mutex<WorkRequestLockMap>>> =
        LazyLock::new(|| Arc::new(Mutex::new(WorkRequestLockMap::new())));

    let request_id = work_request.request_id.clone();

    debug!(request = ?work_request, "Start processing work request");

    let status = &work_request.status;

    if status.is_pending() {
        trace!("Work request is pending");

        status_message
            .update_message("Request is waiting for processing...")
            .await;

        return Ok(());
    }

    if let Some(reason) = status.failed_reason() {
        trace!(?reason, "Work request failed");

        status_message
            .update_message(&format!("Request failed: {}", reason))
            .await;

        return Ok(());
    }

    let Some(progress) = status.progress_info() else {
        trace!(?status, "Work request is not in progress, breaking listen");
        return Ok(());
    };

    if !progress.waiting_for_requester {
        trace!("Work request is not waiting for requester");

        if let Some(message) = progress.message.as_ref() {
            trace!(?message, "Work request has message");
            status_message.update_message(message).await;
        }

        return Ok(());
    }

    let _work_request_guard = match WorkRequestGuard::try_acquire(
        WORK_REQUESTS_PROCESSING_LOCKS.clone(),
        request_id.clone(),
    ) {
        Some(g) => g,
        None => {
            debug!("Work request is already being processed");
            return Ok(());
        }
    };

    let Some(files_data) = progress.files_data.as_ref() else {
        info!(?progress, "Work request has no files, marking as complete");

        status_message
            .update_message("Got no files back from worker")
            .await;

        let res = match RpcClient::work_request_complete(request_id.clone()).await {
            Ok(res) => res,
            Err(e) => {
                status_message
                    .update_message(&format!("Failed to mark request as complete: {}", e))
                    .await;

                return Ok(());
            }
        };

        if !res.is_ok() {
            status_message
                .update_message("Failed to mark request as complete")
                .await;

            return Ok(());
        }

        status_message.delete_message().await;

        return Ok(());
    };

    trace!(?files_data, "Work request has files, downloading...");

    status_message
        .update_message("Downloading media to bot...")
        .await;

    // Limit to max 4 downloads in parallel
    let concurrency_sem = Arc::new(Semaphore::new(4));
    let downloaded_files = files_data
        .iter()
        .map(|x| {
            let concurrency_sem = concurrency_sem.clone();
            async move {
                let _permit = concurrency_sem.acquire().await?;

                let temp_file =
                    tokio::task::spawn_blocking(|| TempFile::new_with_prefix("downloader-bot-dl-"))
                        .await??;

                let tokio_file = tokio::fs::File::from(temp_file.try_clone_file()?);

                trace!(suggested_name = ?x.get_suggested_name(), "Downloading file");

                let (_, suggested_name) = x.download_into(tokio_file).await?;

                Ok::<_, anyhow::Error>((temp_file, suggested_name))
            }
        })
        .map(Box::pin);

    let (downloaded_files, downloaded_files_failed): (Vec<_>, Vec<_>) =
        future::join_all(downloaded_files)
            .await
            .into_iter()
            .partition(std::result::Result::is_ok);

    let downloaded_files = downloaded_files
        .into_iter()
        .map(|x| x.unwrap_or_else(|_| unreachable!()))
        .collect::<Vec<_>>();

    let downloaded_files_failed = downloaded_files_failed
        .into_iter()
        .map(|x| match x {
            Ok(_) => {
                unreachable!()
            }
            Err(e) => e,
        })
        .collect::<Vec<_>>();

    status_message
        .update_message("Files downloaded to bot. Uploading here...")
        .await;

    if let Some(owner_id) = TelegramBot::owner_id()
        && status_message
            .chat_id()
            .as_user()
            .is_some_and(|x| x == owner_id)
    {
        status_message
            .update_message("Copying files to download directory...")
            .await;

        debug!("Copying files to download directory");
        if let Err(e) = copy_files_to_save_dir(&downloaded_files).await {
            status_message
                .send_additional_message(&format!("Failed to copy files: {}", e))
                .await;
        }
        debug!("Copied files to download directory");
    }

    trace!("Chunking files by size");

    let (media_groups, failed_files) = files_to_input_media_groups(
        downloaded_files.iter().map(|(x, _)| x),
        TelegramBot::max_payload_size().bytes().cast_unsigned(),
    )
    .await;

    {
        let errs = {
            let mut errs = vec![];
            for err in downloaded_files_failed {
                errs.push(format!("Failed to download file: {}", err));
            }
            for (_path, err) in failed_files {
                errs.push(format!("Failed to upload file: {}", err));
            }
            for err in work_request.errors.iter() {
                errs.push(err.to_string());
            }
            errs
        };

        if !errs.is_empty() {
            debug!(?errs, "Failed to process some files");
            let mut err_msg = String::new();
            let err_msg = errs.into_iter().fold(&mut err_msg, |acc, e| {
                _ = write!(acc, "\n - {e}");
                acc
            });

            status_message
                .send_additional_message(&format!("Failed to process some files:{}", err_msg))
                .await;
        }
    }

    for media_group in media_groups {
        trace!(?media_group, "Uploading media group");

        let res = TelegramBot::bot()
            .send_media_group(status_message.chat_id(), media_group)
            .reply_parameters(
                ReplyParameters::new(status_message.msg_replying_to_id())
                    .allow_sending_without_reply(),
            )
            .send()
            .await;

        if let Err(e) = res {
            warn!(?e, "Failed to send media group");
            continue;
        }

        trace!("Media group sent");
    }

    status_message.delete_message().await;

    let res = match RpcClient::work_request_complete(request_id.clone()).await {
        Ok(res) => res,
        Err(e) => {
            warn!(?e, "Failed to mark work request as complete");
            return Ok(());
        }
    };

    if !res.is_ok() {
        warn!("Work request marked as complete failed");
        return Ok(());
    }

    info!("Finished processing work request");

    Ok(())
}

pub fn urls_in_message(msg: &TelegramMessage) -> Vec<Url> {
    let entities = msg
        .parse_entities()
        .or_else(|| msg.parse_caption_entities())
        .unwrap_or_default();

    entities
        .iter()
        .filter_map(|x| match x.kind() {
            MessageEntityKind::Url => Url::parse(x.text()).ok(),
            MessageEntityKind::TextLink { url } => Some(url.clone()),
            _ => None,
        })
        .collect()
}
#[tracing::instrument(skip_all)]
async fn copy_files_to_save_dir<T, OT>(
    fixed_file_paths: &[(T, Option<OT>)],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    T: AsRef<std::path::Path> + Sync + Send,
    OT: AsRef<std::path::Path> + Sync + Send,
{
    let download_dir = match TelegramBot::owner_download_dir() {
        Some(x) => x,
        None => return Ok(()),
    };

    for (file, suggested_name) in fixed_file_paths {
        let file = file.as_ref();
        let file_name = suggested_name
            .as_ref()
            .and_then(|n| {
                std::path::PathBuf::from(n.as_ref())
                    .file_name()
                    .map(std::borrow::ToOwned::to_owned)
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
