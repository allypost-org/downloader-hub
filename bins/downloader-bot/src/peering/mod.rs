use std::{sync::Arc, time::Duration};

use app_config::common::{PeerCommsBotConfig, PeerCommsBotTicketConfig};
use app_helpers::futures::retry_future::{RetryConfig, run_retried};
use app_peer_comms::{
    PeeringEndpoint,
    ticket::{
        Ticket,
        targeted::{TargetedTicket, TicketTarget},
    },
};
use tracing::{debug, error, trace};

use crate::peering::jwt::JwtPair;

pub mod jwt;
pub mod rpc;

pub async fn init_peering_endpoint(
    config: PeerCommsBotConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (ticket, refresh_token, token) = run_retried(
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

    let token_pair = if let Some(token) = token {
        JwtPair::new(app_peer_comms::jwt::JwtPair {
            token,
            refresh_token,
        })
    } else {
        jwt::fetch_by_refresh_token(&config.ticket.api.url, refresh_token).await?
    };

    jwt::JwtPair::init(token_pair);
    rpc::RpcClient::init(&config.ticket.api.url);

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

    tokio::task::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_mins(1)).await;

            if let Err(e) = jwt::JwtPair::refresh_via_refresh_token(&config.ticket.api.url).await {
                error!(target: PeeringEndpoint::trace_span_name(), ?e, "Failed to refresh token");
            }
        }
    });

    Ok(())
}

async fn get_ticket(
    config: PeerCommsBotTicketConfig,
) -> Result<(Ticket, Arc<str>, Option<Arc<str>>), Box<dyn std::error::Error + Send + Sync>> {
    #[derive(Debug, serde::Deserialize)]
    struct TicketResp {
        data: TicketRespData,
    }
    #[derive(Debug, serde::Deserialize)]
    struct TicketRespData {
        ticket: String,
        jwt_token: Arc<str>,
        refresh_token: Arc<str>,
    }

    if let Some(ticket_config) = config.ticket {
        trace!(target: PeeringEndpoint::trace_span_name(), ?ticket_config, "Parsing ticket from config");
        let ticket: Ticket =
            TargetedTicket::from_str(&ticket_config.ticket, TicketTarget::Bot).map(Into::into)?;

        let Some(refresh_token) = ticket.refresh_token.clone() else {
            return Err("Ticket must have a refresh token".into());
        };

        return Ok((ticket, refresh_token, ticket_config.jwt_token));
    }

    let api_config = config.api;

    let url = api_config.url.join("/api/v1/join-ticket")?;

    trace!(target: PeeringEndpoint::trace_span_name(), %url, "No ticket provided, fetching from API");

    let res = app_requests::Client::builder()
        .build()?
        .get(url)
        .header("Authorization", format!("Bearer {}", api_config.key))
        .send()
        .await?
        .error_for_status()?
        .json::<TicketResp>()
        .await?;

    let data = res.data;

    trace!(target: PeeringEndpoint::trace_span_name(), ?data, "Parsing ticket from API");

    let ticket = TargetedTicket::from_str(&data.ticket, TicketTarget::Bot).map(Into::into)?;

    Ok((ticket, data.refresh_token, Some(data.jwt_token)))
}
