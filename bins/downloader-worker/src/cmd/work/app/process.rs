use std::{sync::Arc, time::Duration};

use app_actions::{Actions, downloaders::DownloaderError};
use app_helpers::{futures::task_controller::TaskController, temp_dir::TempDir};
use app_peer_comms::{
    AccountPlaceRef, AccountUserRef, IrohBlobStatus, IrohBlobTicket, IrohHash, PeeringEndpoint,
    message::v1::{
        central::work_request::{WorkRequest, WorkRequestInfo, request::WorkRequestMeta},
        common::file::FileReference,
    },
};
use futures::{StreamExt, stream::FuturesUnordered};
use jiff::ToSpan;
use tracing::{Instrument, debug, error, info, trace, warn};

use crate::cmd::work::app::{
    WorkerLoop, broadcaster::Broadcaster,
    helpers::extract_info_request::file_url_to_extract_info_request,
};

pub async fn process_work_request(
    actions: Arc<Actions>,
    peering: Arc<PeeringEndpoint>,
    broadcaster: Broadcaster,
    loop_state: WorkerLoop,
    work_request: WorkRequest,
) {
    let span = tracing::span!(tracing::Level::INFO, "do-request", id = %work_request.request_id());
    let _enter = span.enter();

    let (info, meta) = work_request.into_parts();

    match info {
        WorkRequestInfo::DownloadAndFix(file_reference) => {
            process_download_and_fix(
                actions,
                peering,
                broadcaster,
                loop_state,
                meta,
                file_reference,
            )
            .await;
        }
        WorkRequestInfo::RefreshAccountInfo(_) => {
            warn!(id = %meta.request_id, "worker received account refresh item; refusing");
            match loop_state.rpc() {
                Some(rpc) => {
                    if let Err(e) = rpc.refuse_work_item(meta.request_id).await {
                        error!(?e, "refuse_work_item failed");
                    }
                }
                None => error!("RPC client not connected; cannot refuse work item"),
            }
        }
    }
}

async fn process_download_and_fix(
    actions: Arc<Actions>,
    peering: Arc<PeeringEndpoint>,
    broadcaster: Broadcaster,
    loop_state: WorkerLoop,
    request_meta: WorkRequestMeta,
    file_reference: FileReference,
) {
    let request_id = request_meta.request_id;
    let ordered_by = request_meta.ordered_by;
    let ordered_in = request_meta.ordered_in;

    let tmp_dir = TempDir::in_tmp(format!("downloader-agent.download-and-fix.{request_id}"));
    let tmp_dir = match tmp_dir {
        Ok(x) => x,
        Err(e) => {
            error!(?e, "Failed to create temp dir");
            broadcaster.send_work_request_free(request_id);
            return;
        }
    };

    let timeout = std::time::Duration::from_mins(10);
    let mut tc = TaskController::with_timeout(timeout);

    let res = tc
        .spawn(
            download_and_fix(
                actions,
                peering,
                broadcaster.clone(),
                loop_state,
                request_id.clone(),
                ordered_by,
                ordered_in,
                file_reference,
                tmp_dir,
            )
            .in_current_span(),
        )
        .await;

    match res {
        Ok(Some(())) => {
            info!("Work request completed");
        }
        Ok(None) => {
            warn!(?timeout, "Work request cancelled or timed out");
            broadcaster.send_work_request_free(request_id);
        }
        Err(e) => {
            error!(?e, "Work task failed and probably panicked");
            broadcaster.send_work_request_free(request_id);
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn download_and_fix(
    actions: Arc<Actions>,
    peering: Arc<PeeringEndpoint>,
    broadcaster: Broadcaster,
    loop_state: WorkerLoop,
    request_id: Arc<str>,
    ordered_by: Option<AccountUserRef>,
    ordered_in: Option<AccountPlaceRef>,
    file_reference: FileReference,
    tmp_dir: TempDir,
) {
    info!(?file_reference, "Downloading and fixing");

    match file_reference {
        FileReference::Url(url) => {
            trace!(?request_id, "Downloading files from URL");
            broadcaster.send_work_request_update_status_message(
                request_id.clone(),
                "Downloading files from URL",
            );

            debug!(?url, "Downloading files from URL");

            let url = if let Some(cookie) =
                loop_state.secret_value_for(&url.url, ordered_by.as_ref(), ordered_in.as_ref())
            {
                debug!("Injecting platform cookie for URL");
                url.with_header("cookie", cookie)
            } else {
                url
            };

            let mut paths = Vec::new();
            let mut errs = Vec::new();
            {
                let req = match file_url_to_extract_info_request(&url) {
                    Ok(x) => x,
                    Err(e) => {
                        debug!(?e, "Failed to convert file url to request extract");
                        broadcaster.send_work_request_fail(
                            request_id,
                            &format!("Failed to convert file url to request extract: {e}"),
                        );
                        return;
                    }
                };

                let info = match actions.extract_info(&req).await {
                    Ok(x) => x,
                    Err(e) => {
                        debug!(?e, "Failed to extract info");
                        broadcaster.send_work_request_fail(
                            request_id,
                            &format!("Failed to extract info: {e}"),
                        );
                        return;
                    }
                };

                debug!(
                    urls = ?info.urls.iter().map(|u| u.url.url().as_str()).collect::<Vec<_>>(),
                    "Extracted info"
                );

                let download_requests =
                    app_actions::downloaders::DownloadRequest::from_extracted_info(
                        &info,
                        tmp_dir.path(),
                    );

                debug!(?download_requests, "Download requests");

                let downloaders = actions.enabled_downloaders();
                let download_results = download_requests
                    .into_iter()
                    .map(|x| {
                        x.with_downloader_option(
                            "max-filesize",
                            serde_json::to_value(url.max_filesize).unwrap_or_default(),
                        )
                    })
                    .map(|x| {
                        let downloaders = downloaders.clone();
                        let actions = actions.clone();
                        async move {
                            app_actions::downloaders::download_file_with(
                                actions.ctx(),
                                &downloaders,
                                &x,
                            )
                            .await
                        }
                    })
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
                    broadcaster.send_work_request_add_errors(
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
                    broadcaster.send_work_request_fail(
                        request_id.clone(),
                        &err.clone().original_message(),
                    );
                    return;
                }

                debug!("No files downloaded");
                broadcaster.send_work_request_update_status_message(
                    request_id.clone(),
                    "Got no files from extractor. Refusing so it goes to another worker.",
                );
                tokio::time::sleep(std::time::Duration::from_millis(
                    1000 + rand::random_range(0..3000),
                ))
                .await;
                broadcaster.send_work_request_refuse(request_id.clone());
                return;
            }

            broadcaster.send_work_request_update_status_message(
                request_id.clone(),
                &format!("Downloaded {} file(s) from URL. Fixing...", paths.len()),
            );

            fix_stage_and_deliver(
                actions,
                peering,
                broadcaster,
                request_id,
                paths,
                url.max_filesize,
            )
            .await;
        }
        FileReference::BlobTicket(ticket) => {
            trace!(?request_id, "Downloading files from peer blob ticket");
            broadcaster.send_work_request_update_status_message(
                request_id.clone(),
                "Downloading files from peer",
            );

            let file_name = ticket.file_name.to_string();
            let dest = tmp_dir.path().join(&file_name);
            let mut file = match tokio::fs::File::create(&dest).await {
                Ok(f) => f,
                Err(e) => {
                    error!(?e, "Failed to create temp file for blob download");
                    broadcaster.send_work_request_fail(
                        request_id,
                        &format!("Failed to create temp file: {e}"),
                    );
                    return;
                }
            };

            if let Err(e) = peering.download_ticket_into(ticket.ticket, &mut file).await {
                error!(?e, "Failed to download blob from peer");
                broadcaster
                    .send_work_request_fail(request_id, &format!("Failed to download blob: {e}"));
                return;
            }

            broadcaster.send_work_request_update_status_message(
                request_id.clone(),
                "Downloaded files from peer. Fixing...",
            );

            fix_stage_and_deliver(actions, peering, broadcaster, request_id, vec![dest], None)
                .await;
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn fix_stage_and_deliver(
    actions: Arc<Actions>,
    peering: Arc<PeeringEndpoint>,
    broadcaster: Broadcaster,
    request_id: Arc<str>,
    paths: Vec<std::path::PathBuf>,
    max_filesize: Option<app_config::common::Size>,
) {
    let pe = peering.as_ref();

    let mut fixed_paths = Vec::new();
    let mut exceeded_max_filesize = false;
    {
        let mut errs = Vec::new();
        for path in paths {
            match actions.fix_file(path).await {
                Ok(x) => {
                    let exceeds_limit = if let Some(limit) = max_filesize {
                        match tokio::fs::metadata(&x.file_path).await {
                            Ok(metadata) => metadata.len() > limit.bytes().cast_unsigned(),
                            Err(e) => {
                                errs.push(format!(
                                    "Failed to inspect fixed file {:?}: {e}",
                                    x.file_path
                                ));
                                continue;
                            }
                        }
                    } else {
                        false
                    };

                    if exceeds_limit {
                        exceeded_max_filesize = true;
                        errs.push(
                            DownloaderError::ExceedsMaxFilesize {
                                limit: max_filesize
                                    .expect("exceeded limit requires a configured max filesize"),
                            }
                            .original_message(),
                        );
                    } else {
                        fixed_paths.push(x.file_path);
                    }
                }
                Err(e) => errs.push(e.to_string()),
            }
        }

        if !errs.is_empty() {
            broadcaster.send_work_request_add_errors(request_id.clone(), errs);
        }
    }

    if fixed_paths.is_empty() {
        debug!("No files left to fix");
        let reason = if exceeded_max_filesize {
            DownloaderError::ExceedsMaxFilesize {
                limit: max_filesize.expect("exceeded limit requires a configured max filesize"),
            }
            .original_message()
        } else {
            "No files left to fix".to_string()
        };
        broadcaster.send_work_request_fail(request_id.clone(), &reason);
        return;
    }

    trace!(paths = ?fixed_paths, "Adding paths to blob store");

    let expires = jiff::Timestamp::now()
        .checked_add(30.minutes())
        .expect("30-minute span is always representable as a Timestamp");

    let mut tickets = vec![];
    {
        let batch = match pe.blobs.store().batch().await {
            Ok(x) => x,
            Err(e) => {
                error!(?e, "Failed to create batch");
                broadcaster.send_work_request_free(request_id.clone());
                return;
            }
        };
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
                    broadcaster.send_work_request_add_errors(
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
    }

    let mut served = vec![];
    for ticket in tickets {
        let FileReference::BlobTicket(inner) = &ticket else {
            served.push(ticket);
            continue;
        };
        match wait_until_blob_visible(pe, inner.ticket.hash()).await {
            Ok(()) => served.push(ticket),
            Err(e) => {
                error!(?e, "Imported blob failed to become visible");
                broadcaster.send_work_request_add_errors(
                    request_id.clone(),
                    vec![format!("Failed to process file: {e}")],
                );
            }
        }
    }
    if served.is_empty() {
        let reason = "No files could be stored for delivery".to_string();
        broadcaster.send_work_request_fail(request_id.clone(), &reason);
        return;
    }

    debug!(tickets = ?served, "Added paths to blob store");

    broadcaster.send_work_request_move_to_waiting_for_requester(request_id, served);
}

const BLOB_VISIBILITY_TIMEOUT: Duration = Duration::from_secs(15);
const BLOB_VISIBILITY_POLL: Duration = Duration::from_millis(100);

async fn wait_until_blob_visible(peering: &PeeringEndpoint, hash: IrohHash) -> anyhow::Result<()> {
    let deadline = tokio::time::Instant::now() + BLOB_VISIBILITY_TIMEOUT;
    loop {
        match peering.blobs.store().blobs().status(hash).await {
            Ok(IrohBlobStatus::Complete { .. }) => return Ok(()),
            status => {
                trace!(?hash, ?status, "Blob not visible yet");
            }
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("blob {hash} did not become visible within {BLOB_VISIBILITY_TIMEOUT:?}");
        }
        tokio::time::sleep(BLOB_VISIBILITY_POLL).await;
    }
}
