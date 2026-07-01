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
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            retry_delays: DEFAULT_RETRY_DELAYS.into(),
            reset_retries_after: None,
            jitter: RetryJitter::default(),
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
    let mut tries = 0;
    loop {
        let Some(sleep_time) = config.retry_delays.get(tries) else {
            warn!(
                ?tries,
                "{} has been misbehaving too many times, giving up", name
            );
            return (name, Err("Too many retries".into()));
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

            *sleep_time + jitter
        };

        info!(?tries, "Starting {}", name);

        let started_at = Instant::now();
        match f().await {
            Ok(x) => {
                if return_on_success {
                    return (name, Ok(x));
                }
                info!("{} exited successfully", name);
            }
            Err(e) => {
                warn!(?e, "{} exited with error", name);
            }
        }

        if let Some(reset_retries_after) = config.reset_retries_after
            && started_at.elapsed() >= reset_retries_after
        {
            debug!(
                prev_tries = tries,
                "{} has been behaving {:?}, resetting retry counter", name, reset_retries_after,
            );
            tries = 0;
        }

        tries += 1;
        debug!(
            ?tries,
            ?sleep_time,
            last_ran_ago = ?started_at.elapsed(),
            "Retrying {} after delay",
            name,
        );
        sleep(sleep_time).await;
    }
}
