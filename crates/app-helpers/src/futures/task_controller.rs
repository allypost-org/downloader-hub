use std::{future::Future, time::Duration};

use tokio::{
    sync::{broadcast, broadcast::Sender},
    time::Instant,
};

/// Handles spawning tasks which can also be cancelled by calling `cancel` on the task controller,
/// or by simply dropping it.
///
/// If a [`std::time::Duration`] is supplied using the
/// [`with_timeout`](fn@TaskController::with_timeout) constructor, then any tasks spawned by the
/// [`TaskController`] will automatically be cancelled after the supplied duration has elapsed.
///
/// Cancellation is abrupt: spawned futures are dropped at their next `.await`, so they cannot
/// perform custom asynchronous cleanup. Where graceful shutdown of child futures is required,
/// pass a shutdown signal down and incorporate it into normal program flow instead.
pub struct TaskController {
    timeout: Option<Instant>,
    cancel_sender: Sender<()>,
}

impl TaskController {
    /// Call [`cancel()`](fn@TaskController::cancel) to cancel any tasks spawned by this [`TaskController`]. You can also simply drop
    /// the [`TaskController`] to achieve the same result.
    pub fn cancel(self) {}

    #[must_use]
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1);
        Self {
            timeout: None,
            cancel_sender: tx,
        }
    }

    #[must_use]
    pub fn with_timeout(timeout: Duration) -> Self {
        let (tx, _) = broadcast::channel(1);
        Self {
            timeout: Some(Instant::now() + timeout),
            cancel_sender: tx,
        }
    }

    /// The returned [`JoinHandle`](tokio::task::JoinHandle) resolves to [`None`] if the task was
    /// cancelled instead of running to completion.
    pub fn spawn<T>(&mut self, future: T) -> tokio::task::JoinHandle<Option<T::Output>>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        let mut rx = self.cancel_sender.subscribe();
        if let Some(instant) = self.timeout {
            tokio::task::spawn(async move {
                tokio::select! {
                    res = future => Some(res),
                    _ = rx.recv() => None,
                    () = tokio::time::sleep_until(instant) => None,
                }
            })
        } else {
            tokio::task::spawn(async move {
                tokio::select! {
                    res = future => Some(res),
                    _ = rx.recv() => None,
                }
            })
        }
    }
}

impl Default for TaskController {
    fn default() -> Self {
        Self::new()
    }
}
