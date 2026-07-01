use std::sync::Arc;

use app_peer_comms::message::v1::worker::{CommunicationType, WorkerMessage};
use futures::{SinkExt, stream::SplitSink};
use tokio::{net::TcpStream, sync::RwLock};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::{debug, trace};
use tungstenite as ws;

type Sender = SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, ws::Message>;

pub struct SocketSender {
    inner: Arc<RwLock<Sender>>,
    comm_type: CommunicationType,
}
impl SocketSender {
    pub fn new(sender: Sender, comm_type: CommunicationType) -> Self {
        Self {
            inner: Arc::new(RwLock::new(sender)),
            comm_type,
        }
    }

    pub async fn send_message<T>(&self, msg: T)
    where
        T: Into<WorkerMessage>,
    {
        let _ = self.try_send_message(msg).await;
    }

    pub async fn try_send_message<T>(
        &self,
        msg: T,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
    where
        T: Into<WorkerMessage>,
    {
        let msg: app_peer_comms::Message = msg.into().into();
        trace!(?msg, "Sending message");
        let encoded_msg = self.comm_type.encode(msg)?;
        let msg = match self.comm_type {
            CommunicationType::Json => ws::Message::Text(encoded_msg.try_into()?),
            CommunicationType::Postcard => ws::Message::Binary(encoded_msg.into()),
        };
        self.inner.write().await.send(msg).await.map_err(Into::into)
    }

    pub async fn close(&self, close_frame: Option<ws::protocol::CloseFrame>) {
        debug!(frame = ?close_frame, "Closing connection");
        let mut sender = self.inner.write().await;
        _ = sender.send(ws::Message::Close(close_frame)).await;
        _ = sender.close().await;
        drop(sender);
    }

    pub async fn pong(&self, data: ws::Bytes) {
        debug!(?data, "Ponging");
        _ = self.inner.write().await.send(ws::Message::Pong(data)).await;
    }
}
