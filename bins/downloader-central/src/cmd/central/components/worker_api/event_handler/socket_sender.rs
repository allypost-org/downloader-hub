use std::sync::Arc;

use app_peer_comms::message::v1::worker::CommunicationType;
use axum::extract::ws::{self, WebSocket};
use futures::{SinkExt, stream::SplitSink};
use tokio::sync::RwLock;
use tracing::{debug, trace};

pub type SenderId = Arc<str>;

pub struct SocketSender {
    id: SenderId,
    inner: Arc<RwLock<SplitSink<WebSocket, ws::Message>>>,
    comm_type: CommunicationType,
}
impl SocketSender {
    pub fn new(sender: SplitSink<WebSocket, ws::Message>, comm_type: CommunicationType) -> Self {
        Self {
            id: app_helpers::id::time_thread_id().into(),
            inner: Arc::new(RwLock::new(sender)),
            comm_type,
        }
    }

    pub fn id(&self) -> SenderId {
        self.id.clone()
    }

    pub async fn send_message<T>(
        &self,
        msg: T,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        T: Into<app_peer_comms::message::v1::central::CentralMessage>,
    {
        self.send_message_impl(msg.into().into()).await
    }

    pub async fn send_message_boxed(
        &self,
        msg: Arc<app_peer_comms::message::v1::central::CentralMessage>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let msg: app_peer_comms::Message = msg.as_ref().clone().into();
        self.send_message_impl(msg).await
    }

    pub async fn send_message_impl(
        &self,
        msg: app_peer_comms::Message,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        trace!(?msg, "Sending message");
        let encoded_msg = self.comm_type.encode(msg)?;
        let msg = match self.comm_type {
            CommunicationType::Json => ws::Message::Text(encoded_msg.try_into()?),
            CommunicationType::Postcard => ws::Message::Binary(encoded_msg.into()),
        };
        self.inner.write().await.send(msg).await.map_err(Into::into)
    }

    pub async fn close(&self, close_frame: Option<ws::CloseFrame>) {
        debug!(frame = ?close_frame, "Closing connection");
        let mut sender = self.inner.write().await;
        _ = sender.send(ws::Message::Close(close_frame)).await;
        _ = sender.close().await;
        drop(sender);
    }

    pub async fn pong(&self, data: tungstenite::Bytes) {
        debug!(?data, "Ponging");
        _ = self.inner.write().await.send(ws::Message::Pong(data)).await;
    }
}
