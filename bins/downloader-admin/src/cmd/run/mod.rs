use std::time::Duration;

use app_config::GlobalConfig;
use app_database::Database;
use app_helpers::futures::{
    retry_future::{RetryConfig, keep_running},
    run_future,
};
use tokio::task::JoinSet;
use tracing::{debug, error, info, warn};

use crate::cmd::CmdResult;

pub mod components;
pub mod config;

pub use config::AdminConfig;

const FIVE_MINS: Duration = Duration::from_mins(5);

static RETRY_DELAYS: std::sync::LazyLock<Arc<[Duration]>> = std::sync::LazyLock::new(|| {
    Arc::from([
        Duration::from_millis(300),
        Duration::from_millis(500),
        Duration::from_millis(700),
        Duration::from_secs(1),
        Duration::from_secs(2),
        Duration::from_millis(3500),
        Duration::from_secs(5),
        Duration::from_secs(8),
    ])
});

use std::sync::Arc;

pub fn run(config: AdminConfig) -> CmdResult {
    let Some(x) = run_future(async_run(config)) else {
        debug!("Exited on signal");
        return Ok(());
    };
    x
}

async fn async_run(config: AdminConfig) -> CmdResult {
    Database::init(config.database.clone())
        .await
        .expect("Failed to initialize database");

    let central_slot: Arc<arc_swap::ArcSwapOption<components::CentralClient>> =
        Arc::new(arc_swap::ArcSwapOption::empty());

    let mut handles: JoinSet<(&'static str, components::ComponentResult)> = JoinSet::new();

    let http_config = config.http.clone();
    let session_secret = http_config.session_secret.clone();
    let http_central_slot = central_slot.clone();
    handles.spawn(keep_running(
        "HTTP API",
        Box::new(move || {
            let http_config = http_config.clone();
            let http_central_slot = http_central_slot.clone();
            let session_secret = session_secret.clone();
            Box::pin(async move {
                components::http_api::run(http_config, http_central_slot, session_secret).await
            })
        }),
        RetryConfig::new()
            .with_retry_delays(RETRY_DELAYS.clone())
            .with_reset_retries_after(Some(FIVE_MINS)),
    ));

    let central_config = config.central.clone();
    let central_slot_connect = central_slot.clone();
    handles.spawn(async {
        components::connect_central(central_config, central_slot_connect).await;
        ("Central client", Ok(()))
    });

    while let Some(res) = handles.join_next().await {
        match res {
            Ok((name, Ok(()))) => info!(component = name, "Component exited successfully"),
            Ok((name, Err(e))) => error!(component = name, ?e, "Component failed"),
            Err(e) => error!(?e, "Component task panicked"),
        }
    }

    if let Some(pe) = app_peer_comms::PeeringEndpoint::get_global()
        && let Err(e) = pe.router.shutdown().await
    {
        warn!(?e, "Failed to shutdown peering router");
    }

    Ok(())
}
