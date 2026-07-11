use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use app_config::common::{
    PeerCommsBotConfig, PeerCommsBotTicketConfig, PeerCommsBotTicketFromApiConfig,
};
use app_helpers::futures::retry_future::{RetryConfig, run_retried};
use app_peer_comms::{
    PeeringEndpoint,
    rpc::request::Capabilities,
    ticket::{
        Ticket,
        targeted::{TargetedTicket, TicketTarget},
    },
};
use tracing::{debug, error, trace, warn};

use crate::peering::{reconnect::set_connect_config, rpc::RpcClient};

pub mod reconnect;
pub mod rpc;

static HEARTBEAT: OnceLock<()> = OnceLock::new();

pub async fn init_peering_endpoint(
    config: PeerCommsBotConfig,
    capabilities: Capabilities,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ticket = run_retried(
        "Get ticket",
        Box::new({
            let ticket = config.ticket.clone();
            move || get_ticket(ticket.clone())
        }),
        RetryConfig::new()
            .with_retry_delays(Arc::from([
                Duration::from_millis(300),
                Duration::from_millis(500),
                Duration::from_millis(700),
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_millis(3500),
                Duration::from_secs(5),
                Duration::from_secs(8),
                Duration::from_secs(5),
            ]))
            .with_reset_retries_after(Some(Duration::from_mins(3))),
    )
    .await
    .1?;

    let central_addr = ticket.main.clone();

    debug!(target: PeeringEndpoint::trace_span_name(), ?ticket, "Got ticket");

    let pe = PeeringEndpoint::builder(config.common, ticket.topic_id())
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
        .await?;

    PeeringEndpoint::init(pe)?;

    set_connect_config(config.ticket.api.clone());

    if let Err(e) = RpcClient::init(config.ticket.api.key.clone(), central_addr, capabilities).await
    {
        error!(target: PeeringEndpoint::trace_span_name(), ?e, "Failed to authenticate irpc session");
        return Err(e);
    }

    HEARTBEAT.get_or_init(|| {
        tokio::spawn(async {
            loop {
                let jitter = rand::random_range(0..5_000u64);
                tokio::time::sleep(Duration::from_millis(30_000 + jitter)).await;
                if let Err(e) = RpcClient::heartbeat().await {
                    debug!(?e, "heartbeat failed");
                    if let Err(re) = reconnect().await {
                        warn!(?re, "reconnect failed after heartbeat failure");
                    }
                }
            }
        });
    });

    Ok(())
}

/// Re-export the single-flight reconnect coordinator so existing
/// `crate::peering::reconnect()` call sites keep working while now coalescing
/// concurrent reconnects across all request tasks and the heartbeat.
pub use reconnect::reconnect;

async fn get_ticket(
    config: PeerCommsBotTicketConfig,
) -> Result<Ticket, Box<dyn std::error::Error + Send + Sync>> {
    if let Some(ticket_config) = config.ticket {
        trace!(target: PeeringEndpoint::trace_span_name(), ?ticket_config, "Parsing ticket from config");
        let ticket: Ticket =
            TargetedTicket::from_str(&ticket_config.ticket, TicketTarget::Bot).map(Into::into)?;
        return Ok(ticket);
    }

    fetch_ticket_from_api(config.api).await
}

pub async fn fetch_ticket_from_api(
    api: PeerCommsBotTicketFromApiConfig,
) -> Result<Ticket, Box<dyn std::error::Error + Send + Sync>> {
    Ok(app_peer_comms::ticket::fetch_join_ticket(&api.url, &api.key, TicketTarget::Bot).await?)
}
