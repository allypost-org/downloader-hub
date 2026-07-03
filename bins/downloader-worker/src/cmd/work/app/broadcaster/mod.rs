use std::{
    future::Future,
    sync::{Arc, OnceLock},
    time::Duration,
};

use app_peer_comms::{irpc, message::v1::common::file::FileReference};
use tokio::spawn;
use tracing::{debug, error, warn};

use crate::cmd::work::rpc::RpcClient;

const RETRY_DELAYS: &[Duration] = &[
    Duration::from_millis(200),
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(20),
    Duration::from_secs(30),
];

pub struct Broadcaster;

static BROADCASTER: OnceLock<Broadcaster> = OnceLock::new();

impl Broadcaster {
    pub fn init() {
        _ = BROADCASTER.set(Self);
    }

    #[must_use]
    pub fn get() -> &'static Self {
        BROADCASTER.get().expect("Broadcaster not initialized")
    }
}

#[allow(clippy::unused_self)]
impl Broadcaster {
    pub fn send_work_request_free(&self, request_id: Arc<str>) {
        spawn(deliver("work_request_free", move || {
            let id = request_id.clone();
            async move { RpcClient::work_request_free(id).await.map(drop) }
        }));
    }

    pub fn send_work_request_refuse(&self, request_id: Arc<str>) {
        spawn(deliver("work_request_refuse", move || {
            let id = request_id.clone();
            async move { RpcClient::refuse_work_item(id).await.map(drop) }
        }));
    }

    pub fn send_work_request_update_status_message(&self, request_id: Arc<str>, message: &str) {
        let message: Arc<str> = Arc::from(message);
        spawn(deliver("work_request_update_status", move || {
            let id = request_id.clone();
            let msg = message.clone();
            async move {
                RpcClient::work_request_update_status_message(id, msg)
                    .await
                    .map(drop)
            }
        }));
    }

    pub fn send_work_request_add_errors(&self, request_id: Arc<str>, errors: Vec<String>) {
        spawn(deliver("work_request_add_errors", move || {
            let id = request_id.clone();
            let errs = errors.clone();
            async move { RpcClient::work_request_add_errors(id, errs).await.map(drop) }
        }));
    }

    pub fn send_work_request_move_to_waiting_for_requester(
        &self,
        request_id: Arc<str>,
        files_data: Vec<FileReference>,
    ) {
        spawn(deliver("work_request_move_to_waiting", move || {
            let id = request_id.clone();
            let files = files_data.clone();
            async move {
                RpcClient::work_request_move_to_waiting_for_requester(id, files)
                    .await
                    .map(drop)
            }
        }));
    }

    pub fn send_work_request_fail(&self, request_id: Arc<str>, reason: &str) {
        let reason: Arc<str> = Arc::from(reason);
        spawn(deliver("work_request_fail", move || {
            let id = request_id.clone();
            let r = reason.clone();
            async move { RpcClient::work_request_fail(id, r).await.map(drop) }
        }));
    }
}

async fn deliver<F, Fut>(label: &'static str, f: F)
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), irpc::Error>> + Send + 'static,
{
    for (attempt, delay) in RETRY_DELAYS.iter().enumerate() {
        match f().await {
            Ok(()) => {
                if attempt > 0 {
                    debug!(%label, attempt, "delivered after retry");
                }
                return;
            }
            Err(e) => {
                warn!(%label, attempt, ?e, "deliver failed; retrying");
            }
        }
        #[allow(clippy::cast_possible_truncation)]
        let jitter = Duration::from_millis(rand::random_range(0..=(delay.as_millis() as u64 / 2)));
        tokio::time::sleep(*delay + jitter).await;
    }

    match f().await {
        Ok(()) => {
            debug!(%label, "delivered after final attempt");
        }
        Err(e) => {
            error!(
                %label,
                ?e,
                attempts = RETRY_DELAYS.len() + 1,
                "permanently failed to deliver result; central will rely on takeCleanup"
            );
        }
    }
}
