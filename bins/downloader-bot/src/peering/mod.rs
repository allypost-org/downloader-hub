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
use tracing::{debug, error, info, trace};

use crate::peering::rpc::RpcClient;

pub mod rpc;

static HEARTBEAT: OnceLock<()> = OnceLock::new();

static CONNECT_CONFIG: OnceLock<PeerCommsBotTicketFromApiConfig> = OnceLock::new();

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

    _ = CONNECT_CONFIG.set(config.ticket.api.clone());

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
                }
            }
        });
    });

    Ok(())
}

pub async fn reconnect() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let api = CONNECT_CONFIG
        .get()
        .ok_or("peering not initialized (no connect config)")?;
    info!(target: PeeringEndpoint::trace_span_name(), "Re-bootstrapping: re-fetching join-ticket from central API");
    let ticket = fetch_ticket_from_api(api.clone()).await?;
    let central_addr = ticket.main.clone();
    RpcClient::reauth(central_addr).await?;
    info!(target: PeeringEndpoint::trace_span_name(), "Re-authenticated irpc session against central");
    Ok(())
}

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

async fn fetch_ticket_from_api(
    api: PeerCommsBotTicketFromApiConfig,
) -> Result<Ticket, Box<dyn std::error::Error + Send + Sync>> {
    #[derive(Debug, serde::Deserialize)]
    struct TicketResp {
        data: TicketRespData,
    }
    #[derive(Debug, serde::Deserialize)]
    struct TicketRespData {
        ticket: String,
    }

    let url = api.url.join("/api/v1/join-ticket")?;

    trace!(target: PeeringEndpoint::trace_span_name(), %url, "Fetching ticket from API");

    let res = app_requests::Client::builder()
        .build()?
        .get(url)
        .header("Authorization", format!("Bearer {}", api.key))
        .send()
        .await?
        .error_for_status()?
        .json::<TicketResp>()
        .await?;

    let data = res.data;

    trace!(target: PeeringEndpoint::trace_span_name(), ?data, "Parsing ticket from API");

    Ok(TargetedTicket::from_str(&data.ticket, TicketTarget::Bot).map(Into::into)?)
}
