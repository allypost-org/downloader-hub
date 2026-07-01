use std::sync::Arc;

use app_actions::{
    downloaders::{DownloadRequest, DownloaderError, download_file},
    extractors,
};
use app_helpers::{futures::task_controller::TaskController, temp_dir::TempDir};
use app_peer_comms::{
    IrohBlobTicket, PeeringEndpoint,
    message::v1::{
        central::{
            take_result::TakeResult,
            work_request::{WorkRequest, WorkRequestInfo},
        },
        common::file::FileReference,
    },
};
use futures::{StreamExt, stream::FuturesUnordered};
use tracing::{Instrument, debug, error, info, trace, warn};

use crate::cmd::work::app::{
    IS_PROCESSING, broadcaster::Broadcaster,
    helpers::extract_info_request::file_url_to_extract_info_request,
};

pub async fn handle_take_work_request(resp: TakeResult) {
    let request = match resp {
        TakeResult::Ok(x) => x,
        TakeResult::Err(request_id, err) => {
            info!(msg = %err.msg(), "Failed to take work request");

            super::RECENTLY_HANDLED
                .invalidate(&request_id.to_string())
                .await;

            return;
        }
    };

    let span = tracing::span!(tracing::Level::INFO, "do-request", id = %request.request_id);
    let _enter = span.enter();

    let request_id = request.request_id.clone();

    let processing_permit = match IS_PROCESSING.try_acquire() {
        Ok(x) => x,
        Err(e) => {
            debug!(
                ?e,
                "Some task already in progress, overbooked, rejecting request"
            );

            Broadcaster::get().send_work_request_free(request_id);

            return;
        }
    };

    tokio::task::spawn(
        async move {
            match_request(*request).await;

            drop(processing_permit);
        }
        .in_current_span(),
    );
}

async fn match_request(work_request: WorkRequest) {
    let request_id = work_request.request_id.clone();

    match work_request.info {
        WorkRequestInfo::DownloadAndFix(file_reference) => {
            let tmp_dir =
                TempDir::in_tmp(format!("downloader-agent.download-and-fix.{}", request_id));
            let tmp_dir = match tmp_dir {
                Ok(x) => x,
                Err(e) => {
                    error!(?e, "Failed to create temp dir");
                    Broadcaster::get().send_work_request_free(request_id);
                    return;
                }
            };

            let timeout = chrono::Duration::minutes(10);
            let mut tc = TaskController::with_timeout(
                timeout.to_std().expect("Failed to convert chrono to std"),
            );

            let res = tc
                .spawn(
                    download_and_fix(request_id.clone(), file_reference, tmp_dir).in_current_span(),
                )
                .await;

            match res {
                Ok(Some(())) => {
                    info!("Work request completed");
                    return;
                }
                Ok(None) => {
                    warn!(?timeout, "Work request cancelled or timed out");
                }
                Err(e) => {
                    error!(?e, "Work task failed and probably panicked");
                }
            }

            Broadcaster::get().send_work_request_free(request_id);
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn download_and_fix(request_id: Arc<str>, file_reference: FileReference, tmp_dir: TempDir) {
    info!(?file_reference, "Downloading and fixing");
    let pe = PeeringEndpoint::global();

    match file_reference {
        FileReference::Url(url) => {
            trace!(?request_id, "Downloading files from URL");
            Broadcaster::get().send_work_request_update_status_message(
                request_id.clone(),
                "Downloading files from URL",
            );

            debug!(?url, "Downloading files from URL");

            let mut paths = Vec::new();
            {
                let mut errs = Vec::new();
                let req = match file_url_to_extract_info_request(&url) {
                    Ok(x) => x,
                    Err(e) => {
                        debug!(?e, "Failed to convert file url to request extract");
                        Broadcaster::get().send_work_request_fail(
                            request_id,
                            &format!("Failed to convert file url to request extract: {e}"),
                        );
                        return;
                    }
                };

                let info = match extractors::extract_info(&req).await {
                    Ok(x) => x,
                    Err(e) => {
                        debug!(?e, "Failed to extract info");
                        Broadcaster::get().send_work_request_fail(
                            request_id,
                            &format!("Failed to extract info: {e}"),
                        );
                        return;
                    }
                };

                debug!(?info, "Extracted info");

                let download_requests = DownloadRequest::from_extracted_info(&info, tmp_dir.path());

                debug!(?download_requests, "Download requests");

                let download_results = download_requests
                    .into_iter()
                    .map(|x| x.with_downloader_option("max-filesize", url.max_filesize))
                    .map(|x| async move { download_file(&x).await })
                    .collect::<FuturesUnordered<_>>()
                    .collect::<Vec<_>>()
                    .await;

                debug!(?download_results, "Download results");

                for res in download_results {
                    match res {
                        Ok(x) => paths.push(x.path),
                        Err(e) => errs.push(e),
                    }
                }

                debug!(?paths, ?errs, "Downloaded files from URL");

                let hard_failed_downloaded = errs
                    .into_iter()
                    .filter(|x| !x.is_soft_error())
                    .collect::<Vec<_>>();

                if !hard_failed_downloaded.is_empty() {
                    Broadcaster::get().send_work_request_add_errors(
                        request_id.clone(),
                        hard_failed_downloaded
                            .into_iter()
                            .map(DownloaderError::original_message)
                            .collect(),
                    );
                }
            }

            if paths.is_empty() {
                debug!("No files downloaded");
                Broadcaster::get().send_work_request_update_status_message(
                    request_id.clone(),
                    "Got no files from extractor. Sending back to queue.",
                );
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                Broadcaster::get().send_work_request_free(request_id.clone());
                return;
            }

            Broadcaster::get().send_work_request_update_status_message(
                request_id.clone(),
                &format!("Downloaded {} file(s) from URL. Fixing...", paths.len()),
            );

            let mut fixed_paths = Vec::new();
            {
                let mut errs = Vec::new();
                for path in paths {
                    match app_actions::fix_file(path).await {
                        Ok(x) => fixed_paths.push(x.file_path),
                        Err(e) => errs.push(e.to_string()),
                    }
                }

                if !errs.is_empty() {
                    Broadcaster::get().send_work_request_add_errors(request_id.clone(), errs);
                }
            }

            if fixed_paths.is_empty() {
                debug!("No files left to fix");

                Broadcaster::get()
                    .send_work_request_fail(request_id.clone(), "No files left to fix");

                return;
            }

            trace!(paths = ?fixed_paths, "Adding paths to blob store");

            let mut tickets = vec![];
            let batch = match pe.blobs.store().batch().await {
                Ok(x) => x,
                Err(e) => {
                    error!(?e, "Failed to create batch");
                    Broadcaster::get().send_work_request_free(request_id.clone());
                    return;
                }
            };
            let expires = chrono::Utc::now() + chrono::Duration::minutes(30);
            for path in &fixed_paths {
                let hash_and_fmt = batch
                    .add_path_with_opts(app_peer_comms::IrohAddPathOptions {
                        format: app_peer_comms::IrohBlobFormat::Raw,
                        mode: app_peer_comms::IrohImportMode::Copy,
                        path: path.clone(),
                    })
                    .with_named_tag(PeeringEndpoint::expiring_tag_name(&expires))
                    .await;
                let hash_and_fmt = match hash_and_fmt {
                    Ok(x) => x,
                    Err(e) => {
                        error!(?e, "Failed to add path");
                        Broadcaster::get().send_work_request_add_errors(
                            request_id.clone(),
                            vec![format!("Failed to process file {:?}: {}", path, e)],
                        );
                        continue;
                    }
                };

                let ticket = IrohBlobTicket::new(
                    pe.endpoint_addr().await,
                    hash_and_fmt.hash,
                    hash_and_fmt.format,
                );

                tickets.push(FileReference::BlobTicket(
                    (
                        ticket,
                        path.file_name()
                            .unwrap_or_else(|| path.as_os_str())
                            .to_string_lossy()
                            .to_string(),
                    )
                        .into(),
                ));
            }

            debug!(tickets = ?tickets, "Added paths to blob store");

            Broadcaster::get().send_work_request_move_to_waiting_for_requester(request_id, tickets);
        }
        FileReference::BlobTicket(ticket) => {
            Broadcaster::get().send_work_request_update_status_message(
                request_id.clone(),
                "Downloading files from peer",
            );

            let dl = pe.download(&ticket.ticket);

            let mut stream = match dl.stream().await {
                Ok(x) => x,
                Err(e) => {
                    error!(?e, "Failed to download");
                    return;
                }
            };

            while let Some(it) = stream.next().await {
                dbg!(&it);
            }

            Broadcaster::get().send_work_request_update_status_message(
                request_id.clone(),
                "Downloaded files from peer. Fixing...",
            );
        }
    }
}
