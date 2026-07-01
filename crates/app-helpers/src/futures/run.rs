use std::future::Future;

use tracing::error;

pub fn run_future<F>(fut: F) -> Option<F::Output>
where
    F: Future,
{
    let token = tokio_util::sync::CancellationToken::new();
    #[cfg(not(target_os = "windows"))]
    {
        let token = token.clone();
        let mut signals = signal_hook::iterator::Signals::new(signal_hook::consts::TERM_SIGNALS)
            .expect("Failed to create signals");

        std::thread::spawn(move || {
            if let Some(signal) = signals.forever().next() {
                use tracing::{error, warn};

                warn!("Received signal {signal}, shutting down");
                token.cancel();
                let secs = 8;
                std::thread::sleep(std::time::Duration::from_secs(secs));
                error!(
                    "Could not shut down gracefully after {secs} seconds, forcefully shutting down"
                );
                std::process::exit(1);
            }
        });
    }

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(x) => x,
        Err(err) => {
            error!(?err, "Failed to create tokio runtime");
            std::process::exit(1);
        }
    };

    let res = token.run_until_cancelled(fut);

    runtime.block_on(res)
}
