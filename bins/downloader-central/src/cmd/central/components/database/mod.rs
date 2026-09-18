use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

use app_database::api::log_settings::{LogSettingsScope, resolve_log_settings};
use app_helpers::futures::retry_future::{RetryConfig, keep_running};
use futures::StreamExt;
use tokio::task::JoinSet;
use tracing::{debug, info, trace, warn};

use super::state::SharedCentralState;

static RETRY_DELAYS: LazyLock<Arc<[Duration]>> = LazyLock::new(|| {
    [
        Duration::from_millis(50),
        Duration::from_millis(200),
        Duration::from_millis(500),
        Duration::from_millis(800),
        Duration::from_secs(2),
        Duration::from_secs(5),
        Duration::from_secs(10),
        Duration::from_secs(15),
    ]
    .into()
});

static FIVE_MINS: Duration = Duration::from_mins(5);

pub async fn run(state: SharedCentralState) -> super::ComponentResult {
    info!("Component ready");

    let mut js: JoinSet<(&'static str, super::ComponentResult)> = JoinSet::new();

    js.spawn(keep_running(
        "Database::distributor",
        {
            let state = state.clone();
            Box::new(move || supervise_distributor(state.clone()))
        },
        RetryConfig::new()
            .with_retry_delays(RETRY_DELAYS.clone())
            .with_reset_retries_after(Some(FIVE_MINS)),
    ));

    js.spawn(keep_running(
        "Database::available_work_watcher",
        {
            let state = state.clone();
            Box::new(move || {
                let state = state.clone();
                run_available_work_watcher(state)
            })
        },
        RetryConfig::new()
            .with_retry_delays(RETRY_DELAYS.clone())
            .with_reset_retries_after(Some(FIVE_MINS)),
    ));

    js.spawn(keep_running(
        "Database::log_settings_watcher",
        {
            let state = state.clone();
            Box::new(move || {
                let state = state.clone();
                run_log_settings_watcher(state)
            })
        },
        RetryConfig::new()
            .with_retry_delays(RETRY_DELAYS.clone())
            .with_reset_retries_after(Some(FIVE_MINS)),
    ));

    js.spawn(keep_running(
        "Database::secrets_watcher",
        {
            let state = state.clone();
            Box::new(move || {
                let state = state.clone();
                run_secrets_watcher(state)
            })
        },
        RetryConfig::new()
            .with_retry_delays(RETRY_DELAYS.clone())
            .with_reset_retries_after(Some(FIVE_MINS)),
    ));

    js.spawn(keep_running(
        "Database::revocation_watcher",
        {
            let state = state.clone();
            Box::new(move || {
                let state = state.clone();
                async move {
                    trace!("Starting authed revocation watcher");
                    super::rpc::run_revocation_watcher(state).await
                }
            })
        },
        RetryConfig::new()
            .with_retry_delays(RETRY_DELAYS.clone())
            .with_reset_retries_after(Some(FIVE_MINS)),
    ));

    js.spawn(keep_running(
        "Database::restrictions_watcher",
        {
            let state = state.clone();
            Box::new(move || {
                let state = state.clone();
                async move {
                    trace!("Starting restrictions watcher");
                    super::rpc::run_restrictions_watcher(state).await
                }
            })
        },
        RetryConfig::new()
            .with_retry_delays(RETRY_DELAYS.clone())
            .with_reset_retries_after(Some(FIVE_MINS)),
    ));

    if let Some(res) = js.join_next().await {
        let (name, outcome) = match res {
            Ok((name, Ok(()))) => (name, "exited unexpectedly"),
            Ok((name, Err(e))) => {
                warn!(?e, "{name} task failed; restarting component");
                return Ok(());
            }
            Err(e) => {
                warn!(?e, "Database component task panicked; restarting");
                return Ok(());
            }
        };
        warn!("{name} {outcome}; restarting component");
    }

    Ok(())
}

async fn supervise_distributor(state: SharedCentralState) -> super::ComponentResult {
    let handle = state
        .take_initial_distributor_handle()
        .unwrap_or_else(|| state.respawn_distributor());
    handle.await.map_err(|e| -> super::ComponentError {
        format!("WorkDistributor task ended: {e}").into()
    })?;
    Ok(())
}

async fn run_available_work_watcher(state: SharedCentralState) -> super::ComponentResult {
    trace!("Starting available-work watcher");
    let mut its = state.db().requests_watch_all_available().await?;
    while let Some(emission) = its.next().await {
        match emission {
            Ok(req) => {
                debug!(count = req.len(), "Received available work from db");
                state.distributor().set_available(req).await;
            }
            Err(e) => warn!(?e, "Error reading available work from database"),
        }
    }
    Ok(())
}

async fn run_log_settings_watcher(state: SharedCentralState) -> super::ComponentResult {
    trace!("Starting log-settings watcher");
    let mut settings = state.db().log_settings_watch().await?;
    while let Some(emission) = settings.next().await {
        match emission {
            Ok(rows) => {
                state.set_log_settings(rows.clone()).await;
                let effective = resolve_log_settings(&rows, LogSettingsScope::Central);
                let dynamic = app_logger::LogFilterSettings {
                    console: effective.console,
                    file: effective.file,
                };
                if let Err(e) = app_logger::apply_log_filter_settings(&dynamic) {
                    warn!(
                        ?e,
                        "Invalid dynamic log settings; retaining current filters"
                    );
                }
            }
            Err(e) => warn!(?e, "Error reading log settings from database"),
        }
    }
    Ok(())
}

async fn run_secrets_watcher(state: SharedCentralState) -> super::ComponentResult {
    trace!("Starting secrets watcher");
    let mut stream = state.db().secrets_watch().await?;
    while let Some(emission) = stream.next().await {
        match emission {
            Ok(rows) => state.set_secrets(rows).await,
            Err(e) => warn!(?e, "Error reading secrets from database"),
        }
    }
    Ok(())
}
