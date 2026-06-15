use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

use app_config::common::{DEFAULT_TOPIC_ID, PeerCommsCentralConfig};
use app_helpers::futures::retry_future::{RetryConfig, keep_running};
use app_peer_comms::{PeeringEndpoint, TopicId};
use tokio::task::JoinSet;
use tracing::{debug, trace};

use super::config::CentralConfig;

pub mod _ipc;
pub mod database;
pub mod peers;
pub mod worker_api;

pub async fn spawn(
    config: CentralConfig,
) -> Result<JoinSet<(&'static str, ComponentResult)>, Box<dyn std::error::Error + Send + Sync>> {
    let mut js = JoinSet::new();

    _ipc::init();

    init_peering(config.peer).await?;

    js.spawn(keep_running(
        "Database",
        {
            let db_config = config.database.clone();
            Box::new(move || database::run(db_config.clone()))
        },
        RetryConfig::new()
            .with_retry_delays(RETRY_DELAYS.clone())
            .with_reset_retries_after(Some(FIVE_MINS)),
    ));

    js.spawn(keep_running(
        "Peers",
        Box::new(peers::run),
        RetryConfig::new()
            .with_retry_delays(RETRY_DELAYS.clone())
            .with_reset_retries_after(Some(FIVE_MINS)),
    ));

    js.spawn({
        keep_running(
            "Worker API",
            Box::new(move || worker_api::run(config.worker_api.clone(), config.database.clone())),
            RetryConfig::new()
                .with_retry_delays(RETRY_DELAYS.clone())
                .with_reset_retries_after(Some(FIVE_MINS)),
        )
    });

    js.spawn(async move {
        let mut reciever = super::broadcaster::Broadcaster::recv_from_now();
        while let Ok(msg) = reciever.recv().await {
            trace!(?msg, "Got broadcast from broadcaster");
        }

        ("Broadcaster receiver", Ok(()))
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

    let pe = PeeringEndpoint::builder(config.common, topic_id)
        .build()
        .await?;

    PeeringEndpoint::init(pe)
        .map(|_| ())
        .map_err(std::convert::Into::into)
}

pub type ComponentError = Box<dyn std::error::Error + Send + Sync>;
pub type ComponentResult = Result<(), ComponentError>;
