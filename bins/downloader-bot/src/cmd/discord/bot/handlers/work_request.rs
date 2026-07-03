use std::{
    fmt::Write as _,
    sync::{Arc, LazyLock, Mutex},
};

use app_helpers::temp_file::TempFile;
use app_peer_comms::message::v1::central::work_request::WorkRequest;
use futures::{StreamExt, stream::FuturesUnordered};
use serenity::all::CreateMessage;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, trace, warn};

use crate::{
    cmd::{
        _common::{
            downloadable::Downloadable,
            work_request::{WorkRequestGuard, WorkRequestLockMap},
        },
        discord::bot::{
            discord_bot::DiscordBot,
            helpers::{file_group::send_attachment_groups, status_message::StatusMessage},
        },
    },
    peering::rpc::RpcClient,
};

static WORK_REQUESTS_PROCESSING_LOCKS: LazyLock<Arc<Mutex<WorkRequestLockMap>>> =
    LazyLock::new(|| Arc::new(Mutex::new(WorkRequestLockMap::new())));

pub async fn watch_work_requests() -> Result<(), anyhow::Error> {
    debug!("Starting to watch work requests");
    let mut reqs_it = match RpcClient::work_request_watch_mine_in_progress().await {
        Ok(x) => x,
        Err(e) => {
            error!(?e, "Failed to watch work requests");
            return Err(e.into());
        }
    };

    debug!("Connected to work requests watcher");

    while let Some(snapshot) = match reqs_it.recv().await {
        Ok(x) => x,
        Err(e) => {
            error!(?e, "Got error from work requests watcher");
            return Err(e.into());
        }
    } {
        for req in snapshot.requests.iter().cloned() {
            let req = Arc::new(req);
            let status_message = match StatusMessage::from_metadata(&req.metadata) {
                Ok(x) => x,
                Err(e) => {
                    error!(?e, "Failed to get status message");
                    continue;
                }
            };

            tokio::task::spawn(process_work_request(req.clone(), status_message));
        }
    }

    Ok(())
}

#[tracing::instrument(name = "discord-work-request", skip_all, fields(request_id = ?work_request.request_id))]
#[allow(clippy::too_many_lines)]
pub async fn process_work_request(
    work_request: Arc<WorkRequest>,
    mut status_message: StatusMessage,
) {
    let request_id = work_request.request_id.clone();

    debug!(request = ?work_request, "Start processing work request");

    let status = &work_request.status;

    if status.is_pending() {
        trace!("Work request is pending");
        status_message
            .update_message("Request is waiting for processing...")
            .await;
        return;
    }

    if let Some(reason) = status.failed_reason() {
        trace!(?reason, "Work request failed");
        status_message
            .update_message(&format!("Request failed: {}", reason))
            .await;
        return;
    }

    let Some(progress) = status.progress_info() else {
        trace!(?status, "Work request is not in progress, breaking listen");
        return;
    };

    if !progress.waiting_for_requester {
        trace!("Work request is not waiting for requester");
        if let Some(message) = progress.message.as_ref() {
            trace!(?message, "Work request has message");
            status_message.update_message(message).await;
        }
        return;
    }

    let _work_request_guard = match WorkRequestGuard::try_acquire(
        WORK_REQUESTS_PROCESSING_LOCKS.clone(),
        request_id.clone(),
    ) {
        Some(g) => g,
        None => {
            debug!("Work request is already being processed");
            return;
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
                return;
            }
        };

        if !res.is_ok() {
            status_message
                .update_message("Failed to mark request as complete")
                .await;
            return;
        }

        status_message.delete_message().await;
        return;
    };

    let _concurrency_permit = match DiscordBot::acquire_work_request_permit().await {
        Ok(p) => p,
        Err(e) => {
            warn!(?e, "Work request concurrency semaphore closed");
            return;
        }
    };

    trace!(?files_data, "Work request has files, downloading...");

    status_message
        .update_message("Downloading media to bot...")
        .await;

    let concurrency_sem = Arc::new(Semaphore::new(4));
    let downloaded_futures = files_data.iter().map(|x| {
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
    });

    type DownloadResult = Result<(TempFile, Option<std::path::PathBuf>), anyhow::Error>;

    let downloaded_results: Vec<DownloadResult> = downloaded_futures
        .collect::<FuturesUnordered<_>>()
        .collect()
        .await;

    let (downloaded_files, downloaded_files_failed): (Vec<DownloadResult>, Vec<DownloadResult>) =
        downloaded_results
            .into_iter()
            .partition(std::result::Result::is_ok);

    let downloaded_files = downloaded_files
        .into_iter()
        .map(|x| x.unwrap_or_else(|_| unreachable!()))
        .collect::<Vec<_>>();

    let downloaded_files_failed = downloaded_files_failed
        .into_iter()
        .map(|x| match x {
            Ok(_) => unreachable!(),
            Err(e) => e,
        })
        .collect::<Vec<_>>();

    status_message
        .update_message("Files downloaded to bot. Uploading here...")
        .await;

    if let Some(owner_id) = DiscordBot::owner_id()
        && status_message.author_id() == owner_id
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

    let max_bytes = DiscordBot::max_payload_bytes();

    let failed_files = send_attachment_groups(
        downloaded_files.iter().map(|(f, n)| (f, n.as_ref())),
        max_bytes,
        |group: Vec<serenity::all::CreateAttachment>| {
            trace!(group_len = group.len(), "Uploading attachment group");
            let mut builder =
                CreateMessage::new().reference_message(status_message.reply_reference());
            for att in group {
                builder = builder.add_file(att);
            }
            let channel_id = status_message.channel_id();
            async move {
                if let Err(e) = channel_id.send_message(DiscordBot::bot(), builder).await {
                    warn!(?e, "Failed to send attachment group");
                } else {
                    trace!("Attachment group sent");
                }
            }
        },
    )
    .await;

    {
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

        if !errs.is_empty() {
            debug!(?errs, "Failed to process some files");
            let mut err_msg = String::new();
            errs.into_iter().fold(&mut err_msg, |acc, e| {
                _ = write!(acc, "\n - {e}");
                acc
            });

            status_message
                .send_additional_message(&format!("Failed to process some files:{}", err_msg))
                .await;
        }
    }

    status_message.delete_message().await;

    let res = match RpcClient::work_request_complete(request_id.clone()).await {
        Ok(res) => res,
        Err(e) => {
            warn!(?e, "Failed to mark work request as complete");
            return;
        }
    };

    if !res.is_ok() {
        warn!("Work request marked as complete failed");
        return;
    }

    info!("Finished processing work request");
}

#[tracing::instrument(skip_all)]
async fn copy_files_to_save_dir<T, OT>(
    fixed_file_paths: &[(T, Option<OT>)],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    T: AsRef<std::path::Path> + Sync + Send,
    OT: AsRef<std::path::Path> + Sync + Send,
{
    let download_dir = match DiscordBot::owner_download_dir() {
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
