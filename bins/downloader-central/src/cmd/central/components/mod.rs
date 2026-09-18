use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

use app_config::common::{DEFAULT_TOPIC_ID, PeerCommsCentralConfig};
use app_helpers::futures::retry_future::{RetryConfig, keep_running};
use app_peer_comms::{PeeringEndpoint, TopicId, rpc::RPC_ALPN};
use tokio::task::JoinSet;
use tracing::debug;

use super::config::CentralConfig;
use crate::cmd::central::components::{rpc::CentralRpcServer, state::SharedCentralState};

pub mod database;
pub mod metrics;
pub mod peers;
pub mod rpc;
pub mod state;
pub mod worker_api;

pub async fn spawn(
    config: CentralConfig,
    state: SharedCentralState,
) -> Result<JoinSet<(&'static str, ComponentResult)>, Box<dyn std::error::Error + Send + Sync>> {
    let mut js = JoinSet::new();

    let (handle, join) = rpc::WorkDistributor::spawn(state.db().clone());
    state.set_distributor(handle, join);
    state
        .restrictions
        .store(Some(Arc::new(rpc::RestrictionsManager::new())));

    init_peering(config.peer, state.clone()).await?;

    js.spawn(keep_running(
        "Database",
        {
            let state = state.clone();
            Box::new(move || database::run(state.clone()))
        },
        RetryConfig::new()
            .with_retry_delays(RETRY_DELAYS.clone())
            .with_reset_retries_after(Some(FIVE_MINS)),
    ));

    js.spawn(keep_running(
        "Peers",
        {
            let state = state.clone();
            Box::new(move || peers::run(state.clone()))
        },
        RetryConfig::new()
            .with_retry_delays(RETRY_DELAYS.clone())
            .with_reset_retries_after(Some(FIVE_MINS)),
    ));

    js.spawn({
        let state = state.clone();
        keep_running(
            "Worker API",
            Box::new(move || worker_api::run(config.worker_api.clone(), state.clone())),
            RetryConfig::new()
                .with_retry_delays(RETRY_DELAYS.clone())
                .with_reset_retries_after(Some(FIVE_MINS)),
        )
    });

    Ok(js)
}

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
        Duration::from_secs(30),
        Duration::from_mins(1),
        Duration::from_secs(30),
        Duration::from_mins(1),
    ]
    .into()
});

static FIVE_MINS: Duration = Duration::from_mins(5);

async fn init_peering(
    config: PeerCommsCentralConfig,
    state: SharedCentralState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let topic_id = config.topic_id.map_or_else(
        || {
            let id = TopicId::from_bytes(DEFAULT_TOPIC_ID);
            debug!(?id, "Initialized topic id from default id");
            id
        },
        |x| {
            let id = TopicId::from_bytes(x);
            debug!(?id, "Initialized topic id from config");
            id
        },
    );

    let hook_state = state.clone();
    let pe = PeeringEndpoint::builder(config.common, topic_id)
        .with_router_hook(move |b| b.accept(RPC_ALPN, CentralRpcServer::new(hook_state)))
        .build()
        .await?;

    let pe = Arc::new(pe);
    let _ = state.peering.set(pe);
    Ok(())
}

pub type ComponentError = Box<dyn std::error::Error + Send + Sync>;
pub type ComponentResult = Result<(), ComponentError>;
