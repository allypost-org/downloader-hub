use std::net::SocketAddr;

use app_config::common::{DatabaseConfig, WorkerHttpApiConfig};
use app_database::Database;
use tokio::net::TcpListener;
use tracing::{debug, error, info};

mod routes;

pub async fn run(
    worker_config: WorkerHttpApiConfig,
    database_config: DatabaseConfig,
) -> super::ComponentResult {
    if let Err(e) = Database::init(database_config).await {
        error!(?e, "Database::init failed");
    }

    debug!(?worker_config, "Starting HTTP API");

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

    info!("Component ready");

    info!(
        "Listening on http://{}",
        listener
            .local_addr()
            .expect("Listener has no local address")
    );

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .map_err(|e| {
        error!(?e, "Failed to start server");
        e.into()
    })
}
