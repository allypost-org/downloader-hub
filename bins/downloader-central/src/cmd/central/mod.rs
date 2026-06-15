use app_config::GlobalConfig;
use app_helpers::futures::run_future;
use app_peer_comms::PeeringEndpoint;
use tracing::{Instrument, debug, instrument, warn};

use super::CmdResult;
use crate::cmd::central::config::CentralConfig;

mod auth;
mod broadcaster;
mod components;
pub mod config;
mod rpc_handler;

pub fn run(config: CentralConfig) -> CmdResult {
    let Some(x) = run_future(async_run(config)) else {
        debug!("Exited on signal");
        return Ok(());
    };

    x
}

#[instrument(name = "central", skip_all)]
async fn async_run(config: CentralConfig) -> CmdResult {
    app_database::Database::init(config.database.clone())
        .await
        .expect("Failed to initialize database");

    config::CentralConfig::init_jwt_secret(config.worker_api.jwt_secret.clone());
    if let Err(e) = broadcaster::Broadcaster::init() {
        warn!(?e, "Broadcaster initialization failed");
    }

    let mut handles = components::spawn(config).in_current_span().await?;

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

    if let Some(pe) = PeeringEndpoint::get_global()
        && let Err(e) = pe.router.shutdown().await
    {
        warn!(?e, "Failed to shutdown peering router");
    }

    Ok(())
}
