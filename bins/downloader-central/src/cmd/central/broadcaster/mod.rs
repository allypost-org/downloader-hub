use std::sync::{Arc, OnceLock};

use app_peer_comms::EndpointId;
use tokio::sync::broadcast;
use tracing::trace;

static BROADCASTER: OnceLock<Broadcaster> = OnceLock::new();

type BroadcastMessage = app_peer_comms::message::v1::central::CentralMessage;

pub struct Broadcaster {
    sender: broadcast::Sender<Arc<Broadcast>>,
}

impl Broadcaster {
    pub fn init() -> Result<(), &'static str> {
        let (sender, _) = broadcast::channel::<Arc<Broadcast>>(10);
        BROADCASTER
            .set(Self { sender })
            .map_err(|_| "Failed to init broadcaster")
    }

    pub fn get() -> &'static Self {
        BROADCASTER.get().expect("Broadcaster not initialized")
    }

    pub fn send<T>(msg: T) -> Result<usize, broadcast::error::SendError<Arc<Broadcast>>>
    where
        T: Into<BroadcastMessage>,
    {
        let broadcast = Broadcast::new(Arc::from(msg.into()));

        Self::do_send(broadcast)
    }

    pub fn send_to_audiences<T, I>(
        msg: T,
        audiences: I,
    ) -> Result<usize, broadcast::error::SendError<Arc<Broadcast>>>
    where
        T: Into<BroadcastMessage>,
        I: IntoIterator<Item = BroadcastAudience>,
    {
        let broadcast =
            Broadcast::new(Arc::from(msg.into())).with_audiences(audiences.into_iter().collect());

        Self::do_send(broadcast)
    }

    fn do_send(msg: Broadcast) -> Result<usize, broadcast::error::SendError<Arc<Broadcast>>> {
        trace!(aud = ?msg.audiences, data = ?msg.message, "Sending broadcast");
        let msg = Arc::new(msg);

        Self::get().sender.send(msg)
    }

    pub fn recv_from_now() -> broadcast::Receiver<Arc<Broadcast>> {
        Self::get().sender.subscribe()
    }
}

#[derive(Debug)]
pub struct Broadcast {
    pub message: Arc<BroadcastMessage>,
    pub audiences: Option<Arc<[BroadcastAudience]>>,
}

impl Broadcast {
    #[must_use]
    pub const fn new(message: Arc<BroadcastMessage>) -> Self {
        Self {
            message,
            audiences: None,
        }
    }

    pub fn with_audiences(mut self, audiences: Arc<[BroadcastAudience]>) -> Self {
        self.audiences = Some(audiences);
        self
    }

    pub fn get_if_for_me(
        &self,
        want_audiences: &[BroadcastAudience],
    ) -> Option<Arc<BroadcastMessage>> {
        let Some(target_audiences) = &self.audiences else {
            return Some(self.message.clone());
        };

        let has_match = want_audiences
            .iter()
            .any(|want_audience| target_audiences.contains(want_audience));

        if has_match {
            Some(self.message.clone())
        } else {
            None
        }
    }
}

#[derive(Debug, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BroadcastAudience {
    Socket(Arc<str>),
    Authed(Arc<str>),
    Endpoint(EndpointId),
}

impl PartialEq for BroadcastAudience {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Socket(a), Self::Socket(b)) | (Self::Authed(a), Self::Authed(b)) => a == b,
            (Self::Endpoint(a), Self::Endpoint(b)) => a == b,
            _ => false,
        }
    }
}
