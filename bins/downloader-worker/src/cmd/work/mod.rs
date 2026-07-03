use std::{sync::Arc, time::Duration};

use app_config::common::{
    PeerCommsWorkerConfig, PeerCommsWorkerTicketConfig, PeerCommsWorkerTicketFromApiConfig,
};
use app_helpers::futures::{
    retry_future::{RetryConfig, keep_running, run_retried},
    run_future,
    task_controller::TaskController,
};
use app_peer_comms::{
    PeeringEndpoint,
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
pub mod rpc;

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
        Box::new(move || {
            let conf = conf.clone();
            async move {
                let central_addr = match fetch_ticket_from_api(&conf).await {
                    Ok(t) => t.main,
                    Err(e) => {
                        error!(?e, "Failed to re-fetch join-ticket for worker");
                        return Err(e);
                    }
                };
                app::run(conf, central_addr).await
            }
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
            .with_reset_retries_after(Some(Duration::from_mins(5))),
    )
    .await;

    tc.cancel();

    res
}

async fn init_peering_endpoint(
    config: PeerCommsWorkerConfig,
) -> Result<app_peer_comms::IrohEndpointAddr, super::CmdErr> {
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
                Duration::from_secs(8),
                Duration::from_secs(5),
            ]))
            .with_reset_retries_after(Some(Duration::from_mins(3))),
    )
    .await
    .1?;

    debug!(target: PeeringEndpoint::trace_span_name(), ?ticket, "Got ticket");

    let central_addr = ticket.main.clone();

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

    Ok(central_addr)
}

async fn get_ticket(config: PeerCommsWorkerTicketConfig) -> Result<Ticket, super::CmdErr> {
    if let Some(ticket_config) = config.ticket {
        trace!(target: PeeringEndpoint::trace_span_name(), ?ticket_config, "Parsing ticket from config");
        let ticket: Ticket = TargetedTicket::from_str(&ticket_config.ticket, TicketTarget::Worker)
            .map(Into::into)?;
        return Ok(ticket);
    }

    let api_config = config
        .api
        .as_ref()
        .expect("At least one of `api_url` or `ticket` must be set");

    fetch_ticket_from_api(api_config).await
}

async fn fetch_ticket_from_api(
    api: &PeerCommsWorkerTicketFromApiConfig,
) -> Result<Ticket, super::CmdErr> {
    Ok(app_peer_comms::ticket::fetch_join_ticket(&api.url, &api.key, TicketTarget::Worker).await?)
}
