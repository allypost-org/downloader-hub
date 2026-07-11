use std::{
    collections::HashMap,
    fmt::Write as _,
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::Duration,
};

use app_helpers::temp_file::TempFile;
use app_peer_comms::{
    irpc::channel::mpsc::Receiver,
    message::v1::{
        central::{
            ack_delivery_result::WorkRequestAckResult,
            fail_delivery_result::WorkRequestFailDeliveryResult,
            finish_delivery_result::WorkRequestFinishDeliveryResult,
            release_delivery_result::WorkRequestReleaseDeliveryResult,
            work_request::request::status::WorkRequestStatus,
            work_request_watch_event::WorkRequestWatchEvent,
        },
        common::file::FileReference,
    },
};
use futures::{StreamExt, stream::FuturesUnordered};
use tokio::{
    sync::Mutex,
    task::JoinHandle,
    time::{Instant, timeout_at},
};
use tracing::{debug, info, trace, warn};

use crate::{
    cmd::_common::downloadable::Downloadable,
    peering::{reconnect, rpc::RpcClient},
};

/// Slightly shorter than the 10-minute delivery lease so the bot can release
/// the matching lease itself before the scheduled cleanup normally fires.
const DELIVERY_OPERATION_TIMEOUT: Duration = Duration::from_secs(9 * 60 + 30);
const DELIVERY_LEASE_TIMEOUT: Duration = Duration::from_mins(10);
const MAX_DELIVERY_ATTEMPTS: usize = 5;
const DELIVERY_FAILURE_REASON: &str = "Delivery failed after 5 attempts.";

/// Bounded backoff with jitter between delivery retries (after a release).
const DELIVERY_RETRY_BACKOFF: [Duration; 5] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(20),
];

/// Bounded backoff between delivery finish attempts (while the lease is valid).
const FINISH_RETRY_DELAYS: &[Duration] = &[
    Duration::from_millis(200),
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
];

/// Download concurrency per delivery attempt.
const DOWNLOAD_CONCURRENCY: usize = 4;

// ---------------------------------------------------------------------------
// Platform delivery trait
// ---------------------------------------------------------------------------

/// One platform-owned trait that owns its concrete status-message state and
/// platform delivery behavior. Implemented for each platform's `StatusMessage`
/// type; both implementations are reconstructible from stored request metadata
/// for startup recovery.
///
/// This is task ownership + delivery behavior only — the database `ackDelivery`
/// lease is the real delivery lock. The trait never decides ownership.
pub trait PlatformDelivery: Send + 'static {
    /// Update the status message text (edit/suppress per platform rules).
    fn update_status_message(&mut self, text: &str)
    -> impl std::future::Future<Output = ()> + Send;
    /// Send a supplemental (non-status) message.
    fn send_supplemental_message(&self, text: &str)
    -> impl std::future::Future<Output = ()> + Send;
    /// Delete the status message.
    fn delete_status_message(&self) -> impl std::future::Future<Output = ()> + Send;

    /// True if this request originated from the bot owner (and files should be
    /// copied to the owner download directory).
    fn is_owner_request(&self) -> bool;
    /// Copy the downloaded files to the owner directory, if applicable.
    fn copy_files_to_owner_dir(
        &self,
        files: &[(TempFile, Option<PathBuf>)],
    ) -> impl std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send;

    /// Group downloaded files into platform-sized attachment batches and upload
    /// each batch with the platform reply context. Returns the files that
    /// failed to upload `(path, error)`.
    fn send_batches(
        &self,
        files: &[(TempFile, Option<PathBuf>)],
    ) -> impl std::future::Future<Output = Vec<(Option<PathBuf>, String)>> + Send;
}

// ---------------------------------------------------------------------------
// Keyed task supervisor
// ---------------------------------------------------------------------------

/// Process-local supervisor owning at most one task per request id. Prevents
/// duplicate local watchers from concurrent create responses, startup
/// recovery, and reconnect races. The key is removed when the task exits.
///
/// This is task ownership only; it is NOT the delivery lock and does not
/// replace the database `ackDelivery` mutation.
static SUPERVISOR: OnceLock<RequestSupervisor> = OnceLock::new();

pub struct RequestSupervisor {
    tasks: Mutex<HashMap<Arc<str>, JoinHandle<()>>>,
}

struct ForgetGuard(Arc<str>);

impl Drop for ForgetGuard {
    fn drop(&mut self) {
        let request_id = self.0.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            _ = handle.spawn(async move {
                supervisor().forget(&request_id).await;
            });
        }
    }
}

pub fn supervisor() -> &'static RequestSupervisor {
    SUPERVISOR.get_or_init(|| RequestSupervisor {
        tasks: Mutex::new(HashMap::new()),
    })
}

impl RequestSupervisor {
    /// Request a start for `request_id`. If a task is already running for that
    /// id, this is a no-op; otherwise spawn `task` and record its handle.
    /// The key is removed automatically when the task exits.
    pub async fn start<F>(&self, request_id: Arc<str>, task: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let mut tasks = self.tasks.lock().await;
        if tasks.contains_key(&request_id) {
            trace!(?request_id, "supervisor already has task for request");
            return;
        }
        let key_for_task = request_id.clone();
        let handle = tokio::spawn(async move {
            let _forget = ForgetGuard(key_for_task);
            task.await;
        });
        tasks.insert(request_id, handle);
    }

    async fn forget(&self, request_id: &Arc<str>) {
        let mut tasks = self.tasks.lock().await;
        tasks.remove(request_id);
    }
}

// ---------------------------------------------------------------------------
// Per-request watch + delivery state machine
// ---------------------------------------------------------------------------

/// Run one supervised request task: open a per-request watch and drive it to
/// completion. `is_recovery` marks a startup/recovery task that found the row
/// already `delivering`; such a task stays subscribed through lease expiry
/// instead of exiting when it sees `Delivering` before claiming.
#[tracing::instrument(name = "bot-request-watch", skip_all, fields(?request_id))]
pub async fn watch_and_process<P>(request_id: Arc<str>, mut platform: P, is_recovery: bool)
where
    P: PlatformDelivery,
{
    let mut delivery_attempts: usize = 0;
    let mut stream_failures: usize = 0;

    loop {
        match open_watch(request_id.clone()).await {
            Ok(mut rx) => {
                let outcome = drive_watch(
                    &mut rx,
                    request_id.clone(),
                    &mut platform,
                    is_recovery,
                    &mut delivery_attempts,
                )
                .await;
                match outcome {
                    WatchOutcome::Done | WatchOutcome::Closed => return,
                    WatchOutcome::Reopen => {
                        stream_failures = stream_failures.saturating_add(1);
                        backoff_after_error(stream_failures).await;
                    }
                    WatchOutcome::Reconnect => {
                        if let Err(e) = reconnect().await {
                            warn!(?e, ?request_id, "reconnect failed after watch close");
                        }
                        stream_failures = stream_failures.saturating_add(1);
                        backoff_after_error(stream_failures).await;
                    }
                }
            }
            Err(e) => {
                warn!(?e, ?request_id, "failed to open watch; reconnecting");
                if let Err(re) = reconnect().await {
                    warn!(?re, ?request_id, "reconnect failed after watch open error");
                }
                stream_failures = stream_failures.saturating_add(1);
                backoff_after_error(stream_failures).await;
            }
        }
    }
}

enum WatchOutcome {
    /// Terminal state reached (Done/Failed/Unavailable): task exits.
    Done,
    /// Stream ended cleanly (Ok(None)) without a terminal state: exit.
    Closed,
    /// Reopen the request stream on the current healthy session.
    Reopen,
    /// Transport error: reconnect and reopen.
    Reconnect,
}

#[allow(clippy::too_many_lines)]
async fn drive_watch<P>(
    rx: &mut Receiver<WorkRequestWatchEvent>,
    request_id: Arc<str>,
    platform: &mut P,
    is_recovery: bool,
    delivery_attempts: &mut usize,
) -> WatchOutcome
where
    P: PlatformDelivery,
{
    let watch_id: u64 = rand::random();

    loop {
        let event = match rx.recv().await {
            Ok(Some(event)) => event,
            // stream closed cleanly
            Ok(None) => return WatchOutcome::Closed,
            Err(e) => {
                warn!(
                    ?e,
                    ?request_id,
                    watch_id,
                    "watch stream error; will reconnect"
                );
                return WatchOutcome::Reconnect;
            }
        };

        match event {
            WorkRequestWatchEvent::Request(req) => match req.status() {
                WorkRequestStatus::Pending => {
                    let message = if req.parked() {
                        "All available workers couldn't process this request. It may still be \
                         picked up by another worker."
                    } else {
                        "Request is waiting for processing..."
                    };
                    platform.update_status_message(message).await;
                }
                WorkRequestStatus::InProgress(progress) => {
                    if !progress.waiting_for_requester {
                        if let Some(message) = progress.message.as_ref() {
                            platform.update_status_message(message).await;
                        }
                        // No overall task timeout here; passive observation.
                        continue;
                    }
                    // waitingForRequester: claim a delivery attempt.
                    match claim_and_deliver(
                        request_id.clone(),
                        platform,
                        is_recovery,
                        delivery_attempts,
                    )
                    .await
                    {
                        ClaimOutcome::Finished | ClaimOutcome::Unavailable => {
                            platform.delete_status_message().await;
                            return WatchOutcome::Done;
                        }
                        // Continue: reopen/continue the watch to obtain current state.
                        ClaimOutcome::Continue => {}
                        ClaimOutcome::Reopen => return WatchOutcome::Reopen,
                        ClaimOutcome::Reconnect => return WatchOutcome::Reconnect,
                        ClaimOutcome::Done => return WatchOutcome::Done,
                    }
                }
                WorkRequestStatus::Delivering { .. } => {
                    if is_recovery {
                        // startup/recovery task: stay subscribed through
                        // lease expiry. The matching cleanup/release will
                        // emit the waiting state, then we claim a fresh
                        // attempt. Display recovery status.
                        platform
                            .update_status_message(
                                "Recovering delivery (waiting for prior attempt to clear)...",
                            )
                            .await;
                        continue;
                    }
                    // a normal task that just lost an ack race: exit.
                    debug!(?request_id, "request already delivering; exiting task");
                    return WatchOutcome::Done;
                }
                WorkRequestStatus::Done { .. } => {
                    platform.delete_status_message().await;
                    return WatchOutcome::Done;
                }
                WorkRequestStatus::Failed { reason, .. } => {
                    platform
                        .update_status_message(&format!("Request failed: {reason}"))
                        .await;
                    return WatchOutcome::Done;
                }
            },
            WorkRequestWatchEvent::Unavailable => {
                // non-owner or nonexistent (or deleted mid-watch).
                platform.delete_status_message().await;
                debug!(?request_id, watch_id, "watch unavailable; exiting task");
                return WatchOutcome::Done;
            }
            WorkRequestWatchEvent::Overloaded => {
                warn!(
                    ?request_id,
                    watch_id, "watch overloaded at central; reopening after backoff"
                );
                return WatchOutcome::Reopen;
            }
            WorkRequestWatchEvent::BackendError => {
                warn!(
                    ?request_id,
                    watch_id, "watch backend error; reopening after backoff"
                );
                return WatchOutcome::Reopen;
            }
        }
    }
}

enum ClaimOutcome {
    Finished,
    Unavailable,
    Done,
    Continue,
    Reopen,
    Reconnect,
}

/// Call `WorkRequestAck`, and on `Claimed` run the delivery operation under
/// the shorter-than-lease timeout, then finish. On release/timeout, stay
/// subscribed (return Continue) so the watch picks up the restored waiting
/// state and retries with backoff.
async fn claim_and_deliver<P>(
    request_id: Arc<str>,
    platform: &mut P,
    is_recovery: bool,
    delivery_attempts: &mut usize,
) -> ClaimOutcome
where
    P: PlatformDelivery,
{
    let ack = match RpcClient::work_request_ack(request_id.clone()).await {
        Ok(ack) => ack,
        Err(e) => {
            warn!(
                ?e,
                ?request_id,
                "ack delivery transport error; reconnecting"
            );
            return ClaimOutcome::Reconnect;
        }
    };

    match ack {
        WorkRequestAckResult::Claimed {
            delivery_attempt_id,
            files,
        } => {
            let attempt: Arc<str> = delivery_attempt_id;
            let lease_started_at = Instant::now();
            let operation_deadline = lease_started_at + DELIVERY_OPERATION_TIMEOUT;
            let lease_deadline = lease_started_at + DELIVERY_LEASE_TIMEOUT;

            match timeout_at(
                operation_deadline,
                download_and_deliver(request_id.clone(), files, platform),
            )
            .await
            {
                Ok(()) => {
                    match finish_delivery(request_id.clone(), attempt.clone(), operation_deadline)
                        .await
                    {
                        FinishOutcome::Finished => ClaimOutcome::Finished,
                        FinishOutcome::Unavailable => ClaimOutcome::Unavailable,
                        FinishOutcome::Continue => ClaimOutcome::Reopen,
                        FinishOutcome::Retry => {
                            retry_delivery(
                                request_id,
                                attempt,
                                platform,
                                delivery_attempts,
                                lease_deadline,
                            )
                            .await
                        }
                    }
                }
                Err(_) => {
                    warn!(
                        ?request_id,
                        "delivery operation timed out; retrying delivery"
                    );
                    retry_delivery(
                        request_id,
                        attempt,
                        platform,
                        delivery_attempts,
                        lease_deadline,
                    )
                    .await
                }
            }
        }
        WorkRequestAckResult::AlreadyDelivering => {
            if is_recovery {
                ClaimOutcome::Continue
            } else {
                // race loser: this task's job is done.
                debug!(?request_id, "ack lost race (already delivering)");
                ClaimOutcome::Done
            }
        }
        WorkRequestAckResult::NotWaitingForRequester => {
            // reopen/continue the watch to obtain current state.
            debug!(
                ?request_id,
                "ack not waiting for requester; continuing watch"
            );
            ClaimOutcome::Continue
        }
        WorkRequestAckResult::NotFound => {
            debug!(?request_id, "ack not found; exiting task");
            ClaimOutcome::Unavailable
        }
        WorkRequestAckResult::Unauthorized => {
            warn!(?request_id, "ack unauthorized; exiting task");
            ClaimOutcome::Unavailable
        }
        WorkRequestAckResult::BackendError => {
            warn!(?request_id, "ack backend error; reopening stream");
            ClaimOutcome::Reopen
        }
    }
}

async fn retry_delivery<P>(
    request_id: Arc<str>,
    attempt: Arc<str>,
    platform: &mut P,
    delivery_attempts: &mut usize,
    lease_deadline: Instant,
) -> ClaimOutcome
where
    P: PlatformDelivery,
{
    *delivery_attempts = delivery_attempts.saturating_add(1);
    if *delivery_attempts >= MAX_DELIVERY_ATTEMPTS {
        return match fail_delivery(request_id.clone(), attempt, lease_deadline).await {
            FailDeliveryOutcome::Failed => {
                platform
                    .update_status_message(DELIVERY_FAILURE_REASON)
                    .await;
                ClaimOutcome::Done
            }
            FailDeliveryOutcome::Unavailable => ClaimOutcome::Unavailable,
            FailDeliveryOutcome::Reopen => ClaimOutcome::Reopen,
        };
    }
    if let Err(e) = release_delivery(request_id.clone(), attempt, lease_deadline).await {
        warn!(?e, ?request_id, "failed to release delivery attempt");
    }
    delivery_retry_backoff(*delivery_attempts).await;
    ClaimOutcome::Reopen
}

// ---------------------------------------------------------------------------
// Delivery attempt logic
// ---------------------------------------------------------------------------

/// Download the authoritative (post-ack) files and deliver them through the
/// platform, accumulating per-file failures as a user-visible notice. Must NOT
/// recover files from a pre-ack watch emission. Partial delivery errors are
/// reported to the user; the request is still completed afterwards.
#[tracing::instrument(name = "bot-delivery", skip_all, fields(?request_id))]
pub async fn download_and_deliver<P>(
    request_id: Arc<str>,
    files: Arc<[FileReference]>,
    platform: &mut P,
) where
    P: PlatformDelivery,
{
    if files.is_empty() {
        info!(?request_id, "no files to deliver");
        platform
            .update_status_message("Got no files back from worker")
            .await;
        return;
    }

    platform
        .update_status_message("Downloading media to bot...")
        .await;

    // Bounded-concurrency download. Await/collect the futures before grouping,
    // copying, or uploading (reuse the join_all/FuturesUnordered style).
    let concurrency_sem = Arc::new(tokio::sync::Semaphore::new(DOWNLOAD_CONCURRENCY));
    let downloaded_futures = files.iter().enumerate().map(|(i, x)| {
        let concurrency_sem = concurrency_sem.clone();
        async move {
            let _permit = concurrency_sem.acquire().await?;
            let temp_file =
                tokio::task::spawn_blocking(|| TempFile::new_with_prefix("downloader-bot-dl-"))
                    .await??;
            let tokio_file = tokio::fs::File::from(temp_file.try_clone_file()?);
            let (_, suggested_name) = x.download_into(tokio_file).await?;
            debug!(file_index = i, "downloaded file for delivery");
            Ok::<(TempFile, Option<PathBuf>), anyhow::Error>((temp_file, suggested_name))
        }
    });

    type DownloadResult = Result<(TempFile, Option<PathBuf>), anyhow::Error>;
    let downloaded_results: Vec<DownloadResult> = downloaded_futures
        .collect::<FuturesUnordered<_>>()
        .collect()
        .await;

    let mut downloaded_files = Vec::new();
    let mut download_failures = Vec::new();
    for result in downloaded_results {
        match result {
            Ok(file) => downloaded_files.push(file),
            Err(error) => download_failures.push(error),
        }
    }

    // Copy to owner directory before uploading (preserves current behavior).
    if platform.is_owner_request()
        && let Err(e) = platform.copy_files_to_owner_dir(&downloaded_files).await
    {
        platform
            .send_supplemental_message(&format!("Failed to copy files: {e}"))
            .await;
    }

    // Upload batches through the platform.
    let upload_failures = platform.send_batches(&downloaded_files).await;

    // Accumulate per-file failure notices (download + upload). The request is
    // still completed afterwards (partial-delivery semantics).
    let mut errs: Vec<String> = Vec::new();
    for err in download_failures {
        errs.push(format!("Failed to download file: {err}"));
    }
    for (_path, err) in upload_failures {
        errs.push(format!("Failed to upload file: {err}"));
    }
    if !errs.is_empty() {
        let mut err_msg = String::new();
        for e in &errs {
            _ = write!(err_msg, "\n - {e}");
        }
        platform
            .send_supplemental_message(&format!("Failed to process some files:{err_msg}"))
            .await;
    }
}

// ---------------------------------------------------------------------------
// finish / release helpers
// ---------------------------------------------------------------------------

enum FinishOutcome {
    Finished,
    Unavailable,
    Continue,
    Retry,
}

/// Retry `WorkRequestFinishDelivery` with the reconnect coordinator while the
/// operation lease remains valid. Never finishes with a stale id.
async fn finish_delivery(
    request_id: Arc<str>,
    delivery_attempt_id: Arc<str>,
    deadline: Instant,
) -> FinishOutcome {
    for (attempt, delay) in FINISH_RETRY_DELAYS.iter().enumerate() {
        match timeout_at(
            deadline,
            RpcClient::work_request_finish_delivery(
                request_id.clone(),
                delivery_attempt_id.clone(),
            ),
        )
        .await
        {
            Ok(Ok(WorkRequestFinishDeliveryResult::Ok)) => {
                info!(?request_id, "delivery finished");
                return FinishOutcome::Finished;
            }
            Ok(Ok(
                WorkRequestFinishDeliveryResult::StaleAttempt
                | WorkRequestFinishDeliveryResult::NotDelivering,
            )) => {
                // a newer attempt owns the lease or it was released; cannot
                // finish with this id. Return to the watch for the current state.
                warn!(
                    ?request_id,
                    "finish: stale/not-delivering; returning to watch"
                );
                return FinishOutcome::Continue;
            }
            Ok(Ok(WorkRequestFinishDeliveryResult::NotFound)) => {
                debug!(?request_id, "finish: request not found");
                return FinishOutcome::Unavailable;
            }
            Ok(Ok(WorkRequestFinishDeliveryResult::Unauthorized)) => {
                warn!(?request_id, "finish: unauthorized");
                return FinishOutcome::Unavailable;
            }
            Ok(Ok(WorkRequestFinishDeliveryResult::BackendError)) => {
                warn!(?request_id, attempt, "finish: backend error; retrying");
            }
            Ok(Err(e)) => {
                warn!(
                    ?e,
                    ?request_id,
                    attempt,
                    "finish: transport error; reconnecting"
                );
                if timeout_at(deadline, reconnect()).await.is_err() {
                    return FinishOutcome::Retry;
                }
            }
            Err(_) => return FinishOutcome::Retry,
        }
        if timeout_at(deadline, tokio::time::sleep(*delay))
            .await
            .is_err()
        {
            return FinishOutcome::Retry;
        }
    }
    FinishOutcome::Retry
}

enum FailDeliveryOutcome {
    Failed,
    Unavailable,
    Reopen,
}

async fn fail_delivery(
    request_id: Arc<str>,
    delivery_attempt_id: Arc<str>,
    deadline: Instant,
) -> FailDeliveryOutcome {
    for (attempt, delay) in FINISH_RETRY_DELAYS.iter().enumerate() {
        match timeout_at(
            deadline,
            RpcClient::work_request_fail_delivery(
                request_id.clone(),
                delivery_attempt_id.clone(),
                Arc::from(DELIVERY_FAILURE_REASON),
            ),
        )
        .await
        {
            Ok(Ok(WorkRequestFailDeliveryResult::Failed)) => {
                info!(?request_id, "delivery failed after exhausting attempts");
                return FailDeliveryOutcome::Failed;
            }
            Ok(Ok(
                WorkRequestFailDeliveryResult::StaleAttempt
                | WorkRequestFailDeliveryResult::NotDelivering,
            ))
            | Err(_) => return FailDeliveryOutcome::Reopen,
            Ok(Ok(WorkRequestFailDeliveryResult::NotFound)) => {
                return FailDeliveryOutcome::Unavailable;
            }
            Ok(Ok(WorkRequestFailDeliveryResult::Unauthorized)) => {
                warn!(?request_id, "fail delivery unauthorized");
                return FailDeliveryOutcome::Unavailable;
            }
            Ok(Ok(WorkRequestFailDeliveryResult::BackendError)) => {
                warn!(
                    ?request_id,
                    attempt, "fail delivery backend error; retrying"
                );
            }
            Ok(Err(e)) => {
                warn!(
                    ?e,
                    ?request_id,
                    attempt,
                    "fail delivery transport error; reconnecting"
                );
                if timeout_at(deadline, reconnect()).await.is_err() {
                    return FailDeliveryOutcome::Reopen;
                }
            }
        }
        if timeout_at(deadline, tokio::time::sleep(*delay))
            .await
            .is_err()
        {
            return FailDeliveryOutcome::Reopen;
        }
    }
    FailDeliveryOutcome::Reopen
}

async fn release_delivery(
    request_id: Arc<str>,
    delivery_attempt_id: Arc<str>,
    deadline: Instant,
) -> Result<(), anyhow::Error> {
    for delay in FINISH_RETRY_DELAYS {
        match timeout_at(
            deadline,
            RpcClient::work_request_release_delivery(
                request_id.clone(),
                delivery_attempt_id.clone(),
            ),
        )
        .await
        {
            // Released, stale, not-delivering, not-found, and unauthorized are all
            // harmless here: release may race with cleanup, and the watch picks up
            // the actual state before any new claim.
            Ok(Ok(
                WorkRequestReleaseDeliveryResult::Released
                | WorkRequestReleaseDeliveryResult::StaleAttempt
                | WorkRequestReleaseDeliveryResult::NotDelivering
                | WorkRequestReleaseDeliveryResult::NotFound
                | WorkRequestReleaseDeliveryResult::Unauthorized,
            )) => return Ok(()),
            Ok(Ok(WorkRequestReleaseDeliveryResult::BackendError)) => {
                warn!(?request_id, "release delivery backend error; retrying");
            }
            Ok(Err(e)) => {
                warn!(
                    ?e,
                    ?request_id,
                    "release delivery transport error; reconnecting"
                );
                if timeout_at(deadline, reconnect()).await.is_err() {
                    return Err(anyhow::anyhow!("release delivery reconnect timed out"));
                }
            }
            Err(_) => return Err(anyhow::anyhow!("release delivery lease deadline reached")),
        }
        if timeout_at(deadline, tokio::time::sleep(*delay))
            .await
            .is_err()
        {
            return Err(anyhow::anyhow!("release delivery lease deadline reached"));
        }
    }
    Err(anyhow::anyhow!("release delivery retries exhausted"))
}

// ---------------------------------------------------------------------------
// Watch open + backoffs
// ---------------------------------------------------------------------------

async fn open_watch(
    request_id: Arc<str>,
) -> Result<Receiver<WorkRequestWatchEvent>, anyhow::Error> {
    let watch_id: u64 = rand::random();
    let rx = RpcClient::work_request_wait(request_id, watch_id).await?;
    Ok(rx)
}

async fn backoff_after_error(attempt: usize) {
    let base = Duration::from_millis(500)
        .saturating_mul(u32::try_from(1usize << attempt.min(5)).unwrap_or(u32::MAX));
    let base = base.min(Duration::from_secs(20));
    let jitter = rand::random_range(0..u64::try_from(base.as_millis()).unwrap_or(1).max(1));
    tokio::time::sleep(Duration::from_millis(jitter)).await;
}

async fn delivery_retry_backoff(attempt: usize) {
    let base = DELIVERY_RETRY_BACKOFF
        .get(attempt.saturating_sub(1))
        .copied()
        .unwrap_or_else(|| DELIVERY_RETRY_BACKOFF[DELIVERY_RETRY_BACKOFF.len() - 1]);
    let jitter = rand::random_range(0..u64::try_from(base.as_millis()).unwrap_or(1).max(1));
    tokio::time::sleep(Duration::from_millis(jitter)).await;
}
