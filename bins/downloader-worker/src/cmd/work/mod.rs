use std::{sync::Arc, time::Duration};

use app_config::common::{PeerCommsWorkerConfig, PeerCommsWorkerTicketConfig};
use app_helpers::futures::{
    retry_future::{RetryConfig, keep_running, run_retried},
    run_future,
    task_controller::TaskController,
};
use app_peer_comms::{
    PeeringEndpoint,
    jwt::targeted::TargetedJwtConfig,
    ticket::{
        Ticket,
        targeted::{TargetedTicket, TicketTarget},
    },
};
use app_tasks::TaskRunner;
use config::WorkerConfig;
use tracing::{debug, error, trace};

use super::CmdResult;

pub mod config;
mod global;

mod app;

pub fn run(config: WorkerConfig) -> CmdResult {
    debug!(config = ?config, "Running worker");

    let Some(x) = run_future(async_run(config)) else {
        debug!("Exited on signal");
        return Ok(());
    };

    x
}

async fn async_run(config: WorkerConfig) -> CmdResult {
    _ = app_tasks::config::init(config.task);
    _ = app_helpers::config::init(config.dependency_paths.clone());
    _ = app_actions::config::init(
        config.endpoint,
        config.dependency_paths,
        config.disabled_entries.entries,
        config.request,
    );

    let conf = config.peer.ticket.api.clone().ok_or("No API config")?;
    init_peering_endpoint(config.peer).await?;

    let mut tc = TaskController::new();

    tc.spawn(TaskRunner::run());
    tc.spawn(async move {
        loop {
            match PeeringEndpoint::global().delete_expired_tags().await {
                Ok(x) => {
                    if x > 0 {
                        debug!(count = x, "Deleted expired tags");
                    }
                }
                Err(e) => error!(?e, "Failed to delete expired tags"),
            }

            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });

    let (_, res) = keep_running(
        "Worker",
        Box::new(move || app::run(conf.clone())),
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
                Duration::from_secs(8),
                Duration::from_secs(5),
            ]))
            .with_reset_retries_after(
                TargetedJwtConfig::default_token_expiration_duration()
                    .checked_sub(&chrono::Duration::seconds(30))
                    .and_then(|x| x.to_std().ok()),
            ),
    )
    .await;

    tc.cancel();

    res
}

async fn init_peering_endpoint(config: PeerCommsWorkerConfig) -> CmdResult {
    let (ticket, _refresh_token, _token) = run_retried(
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
                Duration::from_secs(8),
                Duration::from_secs(5),
            ]))
            .with_reset_retries_after(Some(Duration::from_mins(3))),
    )
    .await
    .1?;

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

    Ok(())
}

async fn get_ticket(
    config: PeerCommsWorkerTicketConfig,
) -> Result<(Ticket, Arc<str>, Option<Arc<str>>), super::CmdErr> {
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
        let ticket: Ticket = TargetedTicket::from_str(&ticket_config.ticket, TicketTarget::Worker)
            .map(Into::into)?;

        let Some(refresh_token) = ticket.refresh_token.clone() else {
            return Err(super::CmdErr::from("Ticket must have a refresh token"));
        };

        return Ok((ticket, refresh_token, ticket_config.jwt_token));
    }

    let api_config = config
        .api
        .as_ref()
        .expect("At least one of `api_url` or `ticket` must be set");

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

    let ticket = TargetedTicket::from_str(&data.ticket, TicketTarget::Worker).map(Into::into)?;

    Ok((ticket, data.refresh_token, Some(data.jwt_token)))
}
