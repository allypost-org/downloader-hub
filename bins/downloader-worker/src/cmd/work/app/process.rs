use std::sync::Arc;

use app_actions::{
    downloaders::{DownloadRequest, DownloaderError, download_file},
    extractors,
};
use app_helpers::{futures::task_controller::TaskController, temp_dir::TempDir};
use app_peer_comms::{
    IrohBlobTicket, PeeringEndpoint,
    message::v1::{
        central::work_request::{WorkRequest, WorkRequestInfo, request::WorkRequestMeta},
        common::file::FileReference,
    },
};
use futures::{StreamExt, stream::FuturesUnordered};
use jiff::ToSpan;
use tracing::{Instrument, debug, error, info, trace, warn};

use crate::cmd::work::app::{
    broadcaster::Broadcaster, helpers::extract_info_request::file_url_to_extract_info_request,
};

pub async fn process_work_request(work_request: WorkRequest) {
    let span = tracing::span!(tracing::Level::INFO, "do-request", id = %work_request.request_id());
    let _enter = span.enter();

    let (info, meta) = work_request.into_parts();

    match info {
        WorkRequestInfo::DownloadAndFix(file_reference) => {
            process_download_and_fix(meta, file_reference).await;
        }
        WorkRequestInfo::RefreshAccountInfo(_) => {
            warn!(id = %meta.request_id, "worker received account refresh item; refusing");
            if let Err(e) =
                crate::cmd::work::rpc::RpcClient::refuse_work_item(meta.request_id).await
            {
                error!(?e, "refuse_work_item failed");
            }
        }
    }
}

async fn process_download_and_fix(request_meta: WorkRequestMeta, file_reference: FileReference) {
    let request_id = request_meta.request_id;

    let tmp_dir = TempDir::in_tmp(format!("downloader-agent.download-and-fix.{request_id}"));
    let tmp_dir = match tmp_dir {
        Ok(x) => x,
        Err(e) => {
            error!(?e, "Failed to create temp dir");
            Broadcaster::get().send_work_request_free(request_id);
            return;
        }
    };

    let timeout = std::time::Duration::from_mins(10);
    let mut tc = TaskController::with_timeout(timeout);

    let res = tc
        .spawn(download_and_fix(request_id.clone(), file_reference, tmp_dir).in_current_span())
        .await;

    match res {
        Ok(Some(())) => {
            info!("Work request completed");
        }
        Ok(None) => {
            warn!(?timeout, "Work request cancelled or timed out");
            Broadcaster::get().send_work_request_free(request_id);
        }
        Err(e) => {
            error!(?e, "Work task failed and probably panicked");
            Broadcaster::get().send_work_request_free(request_id);
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn download_and_fix(request_id: Arc<str>, file_reference: FileReference, tmp_dir: TempDir) {
    info!(?file_reference, "Downloading and fixing");

    match file_reference {
        FileReference::Url(url) => {
            trace!(?request_id, "Downloading files from URL");
            Broadcaster::get().send_work_request_update_status_message(
                request_id.clone(),
                "Downloading files from URL",
            );

            debug!(?url, "Downloading files from URL");

            let mut paths = Vec::new();
            let mut errs = Vec::new();
            {
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
                    .map(|x| {
                        x.with_downloader_option(
                            "max-filesize",
                            serde_json::to_value(url.max_filesize).unwrap_or_default(),
                        )
                    })
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

                if errs
                    .iter()
                    .any(app_actions::downloaders::DownloaderError::is_soft_error)
                {
                    warn!(
                        ?request_id,
                        soft_errors = errs.iter().filter(|e| e.is_soft_error()).count(),
                        "Transient (soft) download errors encountered"
                    );
                }

                if !errs.is_empty() {
                    Broadcaster::get().send_work_request_add_errors(
                        request_id.clone(),
                        errs.iter()
                            .cloned()
                            .map(DownloaderError::original_message)
                            .collect(),
                    );
                }
            }

            if paths.is_empty() {
                if let Some(err) = errs.iter().find(|e| e.is_max_filesize()) {
                    Broadcaster::get().send_work_request_fail(
                        request_id.clone(),
                        &err.clone().original_message(),
                    );
                    return;
                }

                debug!("No files downloaded");
                Broadcaster::get().send_work_request_update_status_message(
                    request_id.clone(),
                    "Got no files from extractor. Refusing so it goes to another worker.",
                );
                tokio::time::sleep(std::time::Duration::from_millis(
                    1000 + rand::random_range(0..3000),
                ))
                .await;
                Broadcaster::get().send_work_request_refuse(request_id.clone());
                return;
            }

            Broadcaster::get().send_work_request_update_status_message(
                request_id.clone(),
                &format!("Downloaded {} file(s) from URL. Fixing...", paths.len()),
            );

            fix_stage_and_deliver(request_id, paths).await;
        }
        FileReference::BlobTicket(ticket) => {
            trace!(?request_id, "Downloading files from peer blob ticket");
            Broadcaster::get().send_work_request_update_status_message(
                request_id.clone(),
                "Downloading files from peer",
            );

            let file_name = ticket.file_name.to_string();
            let dest = tmp_dir.path().join(&file_name);
            let mut file = match tokio::fs::File::create(&dest).await {
                Ok(f) => f,
                Err(e) => {
                    error!(?e, "Failed to create temp file for blob download");
                    Broadcaster::get().send_work_request_fail(
                        request_id,
                        &format!("Failed to create temp file: {e}"),
                    );
                    return;
                }
            };

            if let Err(e) = PeeringEndpoint::download_ticket_into(ticket.ticket, &mut file).await {
                error!(?e, "Failed to download blob from peer");
                Broadcaster::get()
                    .send_work_request_fail(request_id, &format!("Failed to download blob: {e}"));
                return;
            }

            Broadcaster::get().send_work_request_update_status_message(
                request_id.clone(),
                "Downloaded files from peer. Fixing...",
            );

            fix_stage_and_deliver(request_id, vec![dest]).await;
        }
    }
}

async fn fix_stage_and_deliver(request_id: Arc<str>, paths: Vec<std::path::PathBuf>) {
    let pe = PeeringEndpoint::global();

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
        Broadcaster::get().send_work_request_fail(request_id.clone(), "No files left to fix");
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
    let expires = jiff::Timestamp::now()
        .checked_add(30.minutes())
        .expect("30-minute span is always representable as a Timestamp");
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
