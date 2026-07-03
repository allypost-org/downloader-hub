use std::{sync::Arc, time::Duration};

use tokio::time::{Instant, sleep};
use tracing::{debug, info, warn};

pub static DEFAULT_RETRY_DELAYS: [Duration; 12] = [
    Duration::from_millis(50),
    Duration::from_millis(200),
    Duration::from_millis(500),
    Duration::from_millis(800),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(15),
    Duration::from_secs(30),
    Duration::from_mins(1),
    Duration::from_secs(30),
    Duration::from_mins(1),
];

pub type RetriedResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub retry_delays: Arc<[Duration]>,
    pub reset_retries_after: Option<Duration>,
    pub jitter: RetryJitter,
    pub max_total_attempts: Option<usize>,
}

impl RetryConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_retry_delays(mut self, retry_delays: Arc<[Duration]>) -> Self {
        self.retry_delays = retry_delays;
        self
    }

    #[must_use]
    pub const fn with_reset_retries_after(mut self, reset_retries_after: Option<Duration>) -> Self {
        self.reset_retries_after = reset_retries_after;
        self
    }

    #[must_use]
    pub const fn with_jitter(mut self, jitter: RetryJitter) -> Self {
        self.jitter = jitter;
        self
    }

    #[must_use]
    pub const fn with_max_total_attempts(mut self, max_total_attempts: Option<usize>) -> Self {
        self.max_total_attempts = max_total_attempts;
        self
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            retry_delays: DEFAULT_RETRY_DELAYS.into(),
            reset_retries_after: None,
            jitter: RetryJitter::default(),
            max_total_attempts: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum RetryJitter {
    None,
    Fraction(u64),
    Fixed(Duration),
}

impl Default for RetryJitter {
    fn default() -> Self {
        Self::Fraction(2)
    }
}

pub async fn keep_running<F, T>(
    name: &'static str,
    f: Box<dyn Fn() -> F + Send + Sync>,
    config: RetryConfig,
) -> (&'static str, RetriedResult<T>)
where
    F: std::future::Future<Output = RetriedResult<T>>,
{
    run_retried_impl(name, f, config, false).await
}

pub async fn run_retried<F, T>(
    name: &'static str,
    f: Box<dyn Fn() -> F + Send + Sync>,
    config: RetryConfig,
) -> (&'static str, RetriedResult<T>)
where
    F: std::future::Future<Output = RetriedResult<T>>,
{
    run_retried_impl(name, f, config, true).await
}

async fn run_retried_impl<F, T>(
    name: &'static str,
    f: Box<dyn Fn() -> F + Send + Sync>,
    config: RetryConfig,
    return_on_success: bool,
) -> (&'static str, RetriedResult<T>)
where
    F: std::future::Future<Output = RetriedResult<T>>,
{
    const TAIL_LOOP: usize = 3;
    const TAIL_WARN_EVERY: usize = 10;

    let mut tries = 0;
    let mut tail_iters: usize = 0;

    loop {
        let sleep_time = match config.retry_delays.len() {
            0 => Duration::ZERO,
            len if tries < len => config.retry_delays[tries],
            len => {
                let tail = len.saturating_sub(TAIL_LOOP);
                let cycle = len - tail;
                let idx = tail + (tries - len) % cycle;
                tail_iters = tail_iters.saturating_add(1);
                if tail_iters == 1 || tail_iters.is_multiple_of(TAIL_WARN_EVERY) {
                    warn!(
                        ?tries,
                        tail_iters,
                        "Backoff schedule exhausted; looping the last {TAIL_LOOP} delays forever"
                    );
                }
                config.retry_delays[idx]
            }
        };
        let sleep_time = {
            #[allow(clippy::cast_possible_truncation)]
            let sleep_time_ms = sleep_time.as_millis() as u64;
            let jitter = match config.jitter {
                RetryJitter::None => Duration::ZERO,
                RetryJitter::Fraction(divisor) => {
                    Duration::from_millis(rand::random_range(0..=(sleep_time_ms / divisor)))
                }
                RetryJitter::Fixed(duration) => duration,
            };

            sleep_time + jitter
        };

        info!(?tries, "Starting {name}");

        let started_at = Instant::now();
        match f().await {
            Ok(x) => {
                if return_on_success {
                    return (name, Ok(x));
                }
                info!("{name} exited successfully");
            }
            Err(e) => {
                warn!(?e, "{name} exited with error");
            }
        }

        if let Some(reset_retries_after) = config.reset_retries_after
            && started_at.elapsed() >= reset_retries_after
        {
            debug!(
                prev_tries = tries,
                "{name} has been behaving {reset_retries_after:?}, resetting retry counter",
            );
            tries = 0;
            tail_iters = 0;
        }

        tries = tries.saturating_add(1);

        if let Some(max) = config.max_total_attempts
            && tries > max
        {
            warn!(
                ?tries,
                max, "{name} exhausted max_total_attempts; giving up"
            );
            return (
                name,
                Err(format!("{name} exhausted max_total_attempts ({max})").into()),
            );
        }

        debug!(
            ?tries,
            ?sleep_time,
            last_ran_ago = ?started_at.elapsed(),
            "Retrying {name} after delay",
        );
        sleep(sleep_time).await;
    }
}
