use std::{sync::Arc, time::Duration};

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

use crate::peering::rpc::RpcClient;

pub mod reconnect;
pub mod rpc;

pub async fn init_peering_endpoint(
    config: PeerCommsBotConfig,
    capabilities: Capabilities,
) -> Result<Arc<RpcClient>, Box<dyn std::error::Error + Send + Sync>> {
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

    let peering = Arc::new(pe);

    let rpc = match RpcClient::connect(
        peering,
        config.ticket.api.clone(),
        central_addr,
        capabilities,
    )
    .await
    {
        Ok(rpc) => rpc,
        Err(e) => {
            error!(target: PeeringEndpoint::trace_span_name(), ?e, "Failed to authenticate irpc session");
            return Err(e);
        }
    };

    spawn_heartbeat(rpc.clone());

    Ok(rpc)
}

fn spawn_heartbeat(rpc: Arc<RpcClient>) {
    tokio::spawn(async move {
        loop {
            let jitter = rand::random_range(0..5_000u64);
            tokio::time::sleep(Duration::from_millis(30_000 + jitter)).await;
            if let Err(e) = rpc.heartbeat().await {
                debug!(?e, "heartbeat failed");
                if let Err(re) = rpc.reconnect().await {
                    warn!(?re, "reconnect failed after heartbeat failure");
                }
                continue;
            }
            match rpc.get_log_settings().await {
                Ok(app_peer_comms::rpc::request::LogSettingsResult::Ok(settings)) => {
                    let settings = app_logger::LogFilterSettings {
                        console: settings.console,
                        file: settings.file,
                    };
                    if let Err(e) = app_logger::apply_log_filter_settings(&settings) {
                        warn!(?e, "Failed to apply dynamic log settings");
                    }
                }
                Ok(result) => debug!(?result, "central did not return log settings"),
                Err(e) => debug!(?e, "log settings request failed"),
            }
        }
    });
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

pub async fn fetch_ticket_from_api(
    api: PeerCommsBotTicketFromApiConfig,
) -> Result<Ticket, Box<dyn std::error::Error + Send + Sync>> {
    Ok(app_peer_comms::ticket::fetch_join_ticket(&api.url, &api.key, TicketTarget::Bot).await?)
}
