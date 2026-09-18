use std::{sync::Arc, time::Duration};

use tokio::sync::watch;
use tracing::{info, warn};

use crate::peering::rpc::RpcClient;

/// Bounded backoff (with jitter applied per attempt) for the reconnect loop.
const RECONNECT_BACKOFF: [Duration; 7] = [
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(20),
    Duration::from_secs(30),
];

/// Single-flight reconnect state owned by [`RpcClient`]. The first caller to
/// arrive performs the bootstrap + reauth; concurrent callers await the same
/// attempt instead of racing independent reconnects.
#[derive(Default)]
pub struct ReconnectCoordinator {
    /// `Some` while a reconnect is in flight; concurrent callers subscribe to
    /// its durable completion result rather than starting a second bootstrap.
    in_flight: std::sync::Mutex<Option<Arc<ReconnectAttempt>>>,
}

struct ReconnectAttempt {
    completion: watch::Sender<Option<Result<(), Arc<str>>>>,
}

impl RpcClient {
    /// Single-flight reconnect. One caller performs the ticket bootstrap and
    /// reauth; other request tasks and the heartbeat await the same attempt.
    /// Failed attempts retry with bounded backoff and jitter until they
    /// succeed - without a session a bot can do nothing, so this does not
    /// give up.
    #[allow(clippy::option_if_let_else)]
    pub async fn reconnect(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (attempt, mut completion, is_leader) = {
            let mut guard = self
                .coordinator
                .in_flight
                .lock()
                .expect("reconnect in_flight lock poisoned");
            let selection = if let Some(attempt) = guard.as_ref() {
                let attempt = Arc::clone(attempt);
                (attempt.clone(), attempt.completion.subscribe(), false)
            } else {
                let (completion_tx, completion) = watch::channel(None);
                let attempt = Arc::new(ReconnectAttempt {
                    completion: completion_tx,
                });
                *guard = Some(attempt.clone());
                (attempt, completion, true)
            };
            drop(guard);
            selection
        };

        if !is_leader {
            if completion.borrow_and_update().is_none() {
                completion
                    .changed()
                    .await
                    .map_err(|_| "reconnect coordinator closed before completion")?;
            }
            return completion
                .borrow_and_update()
                .clone()
                .ok_or_else(|| "reconnect coordinator completed without a result".into())
                .and_then(|result| result.map_err(|error| error.to_string().into()));
        }

        let result = self
            .reconnect_with_backoff()
            .await
            .map_err(|error| Arc::<str>::from(error.to_string()));

        let mut guard = self
            .coordinator
            .in_flight
            .lock()
            .expect("reconnect in_flight lock poisoned");
        if guard
            .as_ref()
            .is_some_and(|in_flight| Arc::ptr_eq(in_flight, &attempt))
        {
            attempt.completion.send_replace(Some(result.clone()));
            guard.take();
        }
        drop(guard);

        result.map_err(|error| error.to_string().into())
    }

    async fn reconnect_with_backoff(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut attempt = 0usize;
        loop {
            match self.try_reconnect_once().await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    warn!(?e, attempt, "reconnect attempt failed; backing off");
                    let base = RECONNECT_BACKOFF[attempt.min(RECONNECT_BACKOFF.len() - 1)];
                    let jitter =
                        rand::random_range(0..u64::try_from(base.as_millis().max(1)).unwrap_or(1));
                    tokio::time::sleep(Duration::from_millis(jitter)).await;
                    attempt = attempt.saturating_add(1);
                }
            }
        }
    }

    async fn try_reconnect_once(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("Re-bootstrapping: re-fetching join-ticket from central API");
        let ticket = crate::peering::fetch_ticket_from_api(self.api.clone()).await?;
        let central_addr = ticket.main.clone();
        self.reauth(central_addr).await?;
        info!("Re-authenticated irpc session against central");
        Ok(())
    }
}
