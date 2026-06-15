use std::sync::{Arc, LazyLock};

use app_config::common::PeerCommsWorkerTicketFromApiConfig;
use app_helpers::futures::task_controller::TaskController;
use app_peer_comms::message::v1::worker::CommunicationType;
use futures::StreamExt;
use socket_sender::SocketSender;
use tracing::{debug, info, instrument, trace};
use tungstenite::client::IntoClientRequest;

use crate::cmd::{CmdResult, work::global::JwtData};

pub(super) mod broadcaster;
pub(super) mod event_handler;
pub(super) mod helpers;
pub(super) mod socket_sender;

pub(super) static IS_PROCESSING: LazyLock<tokio::sync::Semaphore> =
    LazyLock::new(|| tokio::sync::Semaphore::new(1));

#[instrument(name = "worker", skip_all)]
pub async fn run(config: PeerCommsWorkerTicketFromApiConfig) -> CmdResult {
    debug!("Fetching JWTs");
    let jwts = JwtData::fetch_with_api_key(config.url.clone(), config.key).await?;
    trace!(?jwts, "Fetched JWTs");
    JwtData::init_or_update(jwts).await;

    _ = broadcaster::Broadcaster::init();

    let url = {
        let mut url = config.url.join("/api/v1/ws")?;
        let scheme = url.scheme();
        let new = if scheme == "https" { "wss" } else { "ws" };
        url.set_scheme(new).map_err(|()| "Failed to set scheme")?;
        url
    };

    let request = {
        let mut request = url.clone().into_client_request()?;
        request.headers_mut().insert(
            "Authorization",
            format!("Bearer {}", JwtData::get_token().await)
                .parse()
                .expect("Failed to parse header"),
        );
        request.headers_mut().append(
            "Accept",
            "application/postcard"
                .parse()
                .expect("Failed to parse header"),
        );
        request.headers_mut().append(
            "Accept",
            "application/json".parse().expect("Failed to parse header"),
        );
        request
    };

    debug!(url = ?url.as_str(), "Connecting to the API");

    let (stream, _response) = tokio_tungstenite::connect_async(request).await?;

    info!("Connected to the API and ready to receive messages");

    let (sender, receiver) = stream.split();

    let sender = Arc::new(SocketSender::new(sender, CommunicationType::postcard()));

    let mut controller = TaskController::new();

    controller.spawn({
        let sender = sender.clone();
        async move {
            let mut rx = broadcaster::Broadcaster::get().recv();
            while let Ok(msg) = rx.recv().await {
                sender.send_message(msg.as_ref().clone()).await;
            }
        }
    });

    event_handler::handle_socket(receiver, sender.clone()).await?;

    sender.close(None).await;

    Ok(())
}
