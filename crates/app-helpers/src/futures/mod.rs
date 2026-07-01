use std::time::Duration;

pub use run::run_future;
pub use tryhard;

pub mod killable;
pub mod retry_future;
pub mod run;
pub mod task_controller;

pub fn retry_fn<F, Fut, T, E>(
    max_retires: u32,
    f: F,
) -> tryhard::RetryFuture<F, Fut, tryhard::backoff_strategies::ExponentialBackoff, tryhard::NoOnRetry>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    tryhard::retry_fn(f)
        .retries(max_retires)
        .exponential_backoff(Duration::from_millis(50))
        .max_delay(Duration::from_millis(800))
}
