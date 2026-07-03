use std::{collections::VecDeque, sync::Arc};

use app_database::{
    Database,
    api::requests::{RequestInfoResponse, TakeResult as DbTakeResult},
};
use app_peer_comms::{
    irpc::channel::oneshot, message::v1::central::get_work_item_result::GetWorkItemResult,
};
use tokio::sync::mpsc;
use tracing::{debug, error, trace, warn};

use crate::cmd::central::components::metrics;

pub(super) struct Waiter {
    irpc_tx: oneshot::Sender<GetWorkItemResult>,
    authed_id: Arc<str>,
    session_id: u64,
}

pub(super) enum Cmd {
    Available(Arc<[RequestInfoResponse]>),
    GetWorkItem(Waiter),
    Disconnect { session_id: u64 },
}

#[derive(Clone)]
pub struct WorkDistributor {
    cmd_tx: mpsc::Sender<Cmd>,
}

impl WorkDistributor {
    pub fn spawn() -> (Self, tokio::task::JoinHandle<()>) {
        let (cmd_tx, cmd_rx) = mpsc::channel(256);
        let handle = tokio::spawn(run(cmd_rx));
        (Self { cmd_tx }, handle)
    }

    pub async fn set_available(&self, items: Arc<[RequestInfoResponse]>) {
        if self.cmd_tx.send(Cmd::Available(items)).await.is_err() {
            warn!("WorkDistributor gone; dropping available-work snapshot");
        }
    }

    pub async fn park(
        &self,
        authed_id: Arc<str>,
        session_id: u64,
        irpc_tx: oneshot::Sender<GetWorkItemResult>,
    ) {
        if let Err(err) = self
            .cmd_tx
            .send(Cmd::GetWorkItem(Waiter {
                irpc_tx,
                authed_id,
                session_id,
            }))
            .await
        {
            let Cmd::GetWorkItem(waiter) = err.0 else {
                return;
            };
            warn!("WorkDistributor gone; failing getWorkItem with BackendError");
            let _ = waiter.irpc_tx.send(GetWorkItemResult::BackendError).await;
        }
    }

    pub async fn disconnect(&self, session_id: u64) {
        if let Err(e) = self.cmd_tx.send(Cmd::Disconnect { session_id }).await {
            warn!(?e, "WorkDistributor gone; dropping disconnect");
        }
    }
}

async fn run(mut rx: mpsc::Receiver<Cmd>) {
    let mut waiting = VecDeque::new();
    let mut available = Vec::new();

    while let Some(cmd) = rx.recv().await {
        match cmd {
            Cmd::Available(items) => {
                available = items.to_vec();
                distribute(&mut waiting, &mut available).await;
                metrics::set_parked_workers(waiting.len());
            }
            Cmd::GetWorkItem(waiter) => {
                trace!(
                    available = available.len(),
                    parked = waiting.len(),
                    "getWorkItem"
                );
                if let Some(waiter) = hand_to(waiter, &mut available).await {
                    waiting.push_back(waiter);
                    debug!(
                        parked = waiting.len(),
                        "Parked worker (no takeable item right now)"
                    );
                }
                metrics::set_parked_workers(waiting.len());
            }
            Cmd::Disconnect { session_id } => {
                let before = waiting.len();
                waiting.retain(|w| w.session_id != session_id);
                let dropped = before - waiting.len();
                if dropped > 0 {
                    debug!(
                        session_id,
                        dropped, "Dropped parked waiter(s) on disconnect"
                    );
                }
                metrics::set_parked_workers(waiting.len());
            }
        }
    }

    let dropped = waiting.len();
    for waiter in waiting {
        warn!(
            ?waiter.authed_id,
            "Failing parked waiter on distributor shutdown with BackendError"
        );
        let _ = waiter.irpc_tx.send(GetWorkItemResult::BackendError).await;
    }
    if dropped > 0 {
        warn!(dropped, "Failed parked waiters on distributor exit");
    }
    metrics::set_parked_workers(0);
    warn!("WorkDistributor task exited");
}

async fn hand_to(waiter: Waiter, available: &mut Vec<RequestInfoResponse>) -> Option<Waiter> {
    let waiter = waiter;
    while let Some(pos) = available
        .iter()
        .position(|it| !it.refused_by.contains(&waiter.authed_id))
    {
        let item = available.remove(pos);
        let req_id = item.request_id.clone();
        let authed_id = waiter.authed_id.clone();

        match Database::global()
            .requests_take(req_id.clone(), authed_id.clone())
            .await
        {
            Ok(DbTakeResult::Ok(box_req)) => {
                let wr = match box_req.as_ref().try_into() {
                    Ok(w) => w,
                    Err(e) => {
                        error!(?e, ?req_id, "work request convert failed; releasing item");
                        let _ = Database::global().requests_release(req_id, authed_id).await;
                        continue;
                    }
                };
                if waiter
                    .irpc_tx
                    .send(GetWorkItemResult::Ok(Box::new(wr)))
                    .await
                    .is_err()
                {
                    warn!(?req_id, "Worker vanished during handoff; releasing item");
                    let _ = Database::global().requests_release(req_id, authed_id).await;
                }
                metrics::work_item_dispatched();
                return None;
            }
            _ => {
                debug!(?req_id, "take failed (race/done); trying next item");
            }
        }
    }
    Some(waiter)
}

async fn distribute(waiting: &mut VecDeque<Waiter>, available: &mut Vec<RequestInfoResponse>) {
    loop {
        if available.is_empty() || waiting.is_empty() {
            return;
        }
        let idx = waiting.iter().position(|w| {
            available
                .iter()
                .any(|it| !it.refused_by.contains(&w.authed_id))
        });
        let Some(idx) = idx else {
            return;
        };
        let waiter = waiting.remove(idx).expect("positioned index");
        if let Some(waiter) = hand_to(waiter, available).await {
            waiting.push_back(waiter);
            return;
        }
    }
}
