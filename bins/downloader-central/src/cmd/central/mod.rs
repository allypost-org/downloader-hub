use std::sync::Arc;

use app_helpers::futures::run_future;
use tracing::{Instrument, debug, instrument, warn};

use super::CmdResult;
use crate::cmd::central::config::CentralConfig;

mod components;
pub mod config;

pub fn run(config: CentralConfig) -> CmdResult {
    let Some(x) = run_future(async_run(config)) else {
        debug!("Exited on signal");
        return Ok(());
    };

    x
}

#[instrument(name = "central", skip_all)]
async fn async_run(config: CentralConfig) -> CmdResult {
    let state = components::state::CentralState::shared();

    let db = Arc::new(
        app_database::Database::new(config.database.clone())
            .await
            .expect("Failed to initialize database"),
    );
    let _ = state.db.set(db);

    let mut handles = components::spawn(config, state.clone())
        .in_current_span()
        .await?;

    while let Some(res) = handles.join_next().await {
        let (name, res) = match res {
            Ok(x) => x,
            Err(e) => {
                warn!(?e, "Component task panicked");
                continue;
            }
        };

        if let Err(e) = res {
            warn!(?name, ?e, "Component exited with error");
            continue;
        }

        debug!(?name, "Component task exited normally");
    }

    if let Some(pe) = state.peering.get()
        && let Err(e) = pe.router.shutdown().await
    {
        warn!(?e, "Failed to shutdown peering router");
    }

    Ok(())
}
