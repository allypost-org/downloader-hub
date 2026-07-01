use std::net::SocketAddr;

use app_config::common::{DatabaseConfig, WorkerHttpApiConfig};
use app_database::Database;
use app_helpers::futures::killable::spawn_killable;
use app_peer_comms::message::v1::central::CentralMessage;
use tokio::net::TcpListener;
use tracing::{debug, error, info, trace};

use crate::cmd::central::{broadcaster::Broadcaster, components::_ipc::IpcMessage};

mod auth;
mod event_handler;
mod global;
mod request;
mod routes;

pub async fn run(
    worker_config: WorkerHttpApiConfig,
    database_config: DatabaseConfig,
) -> super::ComponentResult {
    _ = Database::init(database_config).await;
    global::GlobalData::init(global::GlobalData {
        jwt_secret: worker_config.jwt_secret.clone(),
    });

    debug!(?worker_config, "Starting server");

    let app = routes::create_router(&routes::RouterConfig {
        request_ip_source: worker_config.request_ip_source.parse()?,
    });

    let listener = match TcpListener::bind(worker_config.bind_addr()).await {
        Ok(listener) => listener,
        Err(e) => {
            error!(?e, "Failed to bind to worker API");

            return Err(e.into());
        }
    };

    let broadcast_task_killer = spawn_killable(run_broadcast_forwarder());

    info!("Component ready");

    IpcMessage::WorkerApiReady.send()?;

    info!(
        "Listening on http://{}",
        listener
            .local_addr()
            .expect("Listener has no local address")
    );

    let res = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .map_err(|e| {
        error!(?e, "Failed to start server");
        e.into()
    });

    broadcast_task_killer.kill();

    res
}

#[tracing::instrument(skip_all)]
pub async fn run_broadcast_forwarder() {
    let mut receiver = IpcMessage::recv_from_now();

    while let Ok(msg) = receiver.recv().await {
        match msg.as_ref() {
            IpcMessage::WorkerRequests(requests) => {
                trace!(?requests, "Received worker requests");
                let msg = CentralMessage::work_requests(requests.iter().cloned());
                let msg = match msg {
                    Ok(x) => x,
                    Err(e) => {
                        error!(?e, "Failed to serialize broadcast");
                        continue;
                    }
                };
                if let Err(e) = Broadcaster::send(msg) {
                    error!(?e, "Failed to send broadcast");
                    continue;
                }

                trace!("Broadcast worker requests");
            }
            _ => {
                trace!(?msg, "Ignoring IPC message");
            }
        }
    }
}
