use std::{sync::Arc, time::Duration};

use app_config::common::PeerCommsAdminConfig;
use app_helpers::futures::retry_future::RetryConfig;
use app_peer_comms::{
    PeeringEndpoint, irpc_iroh,
    rpc::{AuthResult, CentralProtocol, request},
    ticket::{self, targeted::TicketTarget},
};
use arc_swap::ArcSwapOption;
use tokio::time::sleep;
use tracing::{debug, info, warn};

pub mod central;
pub mod http_api;

pub type ComponentError = Box<dyn std::error::Error + Send + Sync>;
pub type ComponentResult = Result<(), ComponentError>;

pub use central::CentralClient;

const RECONNECT_BACKOFF: [Duration; 6] = [
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(15),
    Duration::from_secs(30),
    Duration::from_mins(1),
];

const LIVENESS_PROBE_INTERVAL: Duration = Duration::from_secs(15);
const LIVENESS_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Supervised central client: dials central, authenticates as Admin, and
/// **reconnects automatically** when the connection drops. Exits permanently
/// only when there is no central config or the admin API key is rejected
/// (`AuthResult::Unauthorized`) — so a bad key never hammers central.
pub async fn connect_central(
    config: PeerCommsAdminConfig,
    central_slot: Arc<ArcSwapOption<CentralClient>>,
) {
    let Some(api) = config.api.clone() else {
        info!("No central API config provided; admin will run in DB-only mode");
        return;
    };

    let mut backoff_idx: usize = 0;
    loop {
        let outcome = establish_once(&config, &api, &central_slot).await;

        let permanent = match &outcome {
            EstablishOutcome::Unauthorized => true,
            EstablishOutcome::Transient(e) => {
                warn!(?e, "central connect failed; backing off before retry");
                false
            }
            EstablishOutcome::Lost => {
                info!("central connection lost; reconnecting");
                false
            }
        };

        if permanent {
            return;
        }

        let delay = RECONNECT_BACKOFF[backoff_idx.min(RECONNECT_BACKOFF.len() - 1)];
        backoff_idx = backoff_idx.saturating_add(1);
        debug!(?delay, "sleeping before central reconnect attempt");
        sleep(delay).await;
    }
}

enum EstablishOutcome {
    Unauthorized,
    Transient(Box<dyn std::error::Error + Send + Sync>),
    Lost,
}

async fn establish_once(
    config: &PeerCommsAdminConfig,
    api: &app_config::common::PeerCommsAdminApiConfig,
    central_slot: &Arc<ArcSwapOption<CentralClient>>,
) -> EstablishOutcome {
    let ticket = match fetch_ticket(api).await {
        Ok(t) => t,
        Err(e) => return EstablishOutcome::Transient(e),
    };

    debug!(?ticket, "Got admin join ticket");

    let central_addr = ticket.main.clone();
    let pe = match PeeringEndpoint::builder(config.common.clone(), ticket.topic_id())
        .with_main_node(Some(ticket.main.id))
        .with_peers(
            ticket
                .peers()
                .iter()
                .cloned()
                .chain([ticket.main])
                .collect(),
        )
        .with_refresh_url(ticket.refresh_url)
        .build()
        .await
    {
        Ok(pe) => pe,
        Err(e) => {
            return EstablishOutcome::Transient(
                format!("Failed to build peering endpoint: {e:?}").into(),
            );
        }
    };

    if let Err(e) = PeeringEndpoint::init(pe) {
        warn!(?e, "PeeringEndpoint already initialized; reusing existing");
    }

    let client = irpc_iroh::client::<CentralProtocol>(
        PeeringEndpoint::global().router.endpoint().clone(),
        central_addr.clone(),
        app_peer_comms::rpc::RPC_ALPN,
    );

    let auth = client
        .rpc(request::Auth {
            api_key: api.key.clone(),
            capabilities: request::Capabilities::Admin,
            version: crate::config::Config::app_version().to_string(),
        })
        .await;

    let info = match auth {
        Ok(AuthResult::Ok(info)) => info,
        Ok(AuthResult::Unauthorized) => {
            warn!("Central rejected the admin API key (unauthorized); not retrying");
            return EstablishOutcome::Unauthorized;
        }
        Err(e) => {
            return EstablishOutcome::Transient(format!("irpc Auth failed: {e:?}").into());
        }
    };

    info!(?info, "irpc admin session established");
    let central = Arc::new(CentralClient::new(Arc::new(client), api.clone()));
    central_slot.store(Some(central.clone()));

    watch_until_lost(&central).await;

    central_slot.store(None);
    EstablishOutcome::Lost
}

async fn fetch_ticket(
    api: &app_config::common::PeerCommsAdminApiConfig,
) -> Result<app_peer_comms::ticket::Ticket, Box<dyn std::error::Error + Send + Sync>> {
    let retry_cfg = RetryConfig::new().with_max_total_attempts(Some(5));
    let (_, res) = app_helpers::futures::retry_future::run_retried(
        "Fetch admin join ticket",
        Box::new({
            let url = api.url.clone();
            let key = api.key.clone();
            move || {
                let url = url.clone();
                let key = key.clone();
                Box::pin(async move {
                    ticket::fetch_join_ticket(&url, &key, TicketTarget::Admin)
                        .await
                        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                            format!("Failed to fetch join ticket: {e:?}").into()
                        })
                })
            }
        }),
        retry_cfg,
    )
    .await;
    res.map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e })
}

/// Periodically probe central until the connection looks dead (probe fails or
/// times out N times in a row), then return so the outer loop reconnects.
async fn watch_until_lost(central: &Arc<CentralClient>) {
    const FAILURE_THRESHOLD: u32 = 3;
    let mut consecutive_failures: u32 = 0;
    loop {
        sleep(LIVENESS_PROBE_INTERVAL).await;
        let probe = tokio::time::timeout(LIVENESS_PROBE_TIMEOUT, central.get_capabilities()).await;
        match probe {
            Ok(Ok(_)) => {
                if consecutive_failures > 0 {
                    debug!("central liveness probe recovered");
                }
                consecutive_failures = 0;
            }
            Ok(Err(e)) => {
                consecutive_failures = consecutive_failures.saturating_add(1);
                warn!(?e, consecutive_failures, "central liveness probe failed");
            }
            Err(_) => {
                consecutive_failures = consecutive_failures.saturating_add(1);
                warn!(consecutive_failures, "central liveness probe timed out");
            }
        }
        if consecutive_failures >= FAILURE_THRESHOLD {
            warn!(
                consecutive_failures,
                "central liveness failure threshold reached; dropping client"
            );
            return;
        }
    }
}
