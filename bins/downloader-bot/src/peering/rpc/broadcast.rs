use std::sync::{Arc, OnceLock};

use app_peer_comms::message::v1::bot::BotMessage;
use tokio::sync::broadcast;
use tracing::trace;

static BROADCASTER: OnceLock<RpcBroadcaster> = OnceLock::new();

type Broadcast = Arc<BotMessage>;

pub struct RpcBroadcaster {
    send: broadcast::Sender<Broadcast>,
}

impl RpcBroadcaster {
    pub fn new() -> Self {
        Self {
            send: broadcast::channel::<Broadcast>(512).0,
        }
    }

    pub fn init() -> Result<(), &'static str> {
        BROADCASTER
            .set(Self::new())
            .map_err(|_| "Failed to init broadcaster")
    }

    pub fn get() -> &'static Self {
        BROADCASTER.get().expect("Broadcaster not initialized")
    }

    pub fn send<T>(&self, msg: T)
    where
        T: Into<Broadcast>,
    {
        _ = self.try_send(msg);
    }

    pub fn try_send<T>(&self, msg: T) -> Result<usize, broadcast::error::SendError<Broadcast>>
    where
        T: Into<Broadcast>,
    {
        let msg = msg.into();

        trace!(?msg, "Broadcasting message");

        self.send.send(msg)
    }

    pub fn recv(&self) -> broadcast::Receiver<Broadcast> {
        self.send.subscribe()
    }
}
