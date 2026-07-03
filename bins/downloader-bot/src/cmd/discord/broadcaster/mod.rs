use std::sync::{Arc, OnceLock};

use serenity::all::{ChannelId, CreateMessage, EditMessage, Message, ReactionType};
use tokio::sync::broadcast;
use tracing::trace;

static BROADCASTER: OnceLock<MessageBroadcaster> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct Broadcast {
    pub data: Arc<BroadcastData>,
}

impl Broadcast {
    pub fn from_data<T>(data: T) -> Self
    where
        T: Into<BroadcastData>,
    {
        Self {
            data: Arc::new(data.into()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum BroadcastData {
    Global(CreateMessage),
    ToChannel(ChannelId, CreateMessage),
    Reply(Message, CreateMessage),
    Edit(Message, EditMessage),
    Reaction(Message, ReactionType),
}

impl From<CreateMessage> for BroadcastData {
    fn from(value: CreateMessage) -> Self {
        Self::Global(value)
    }
}

impl From<(ChannelId, CreateMessage)> for BroadcastData {
    fn from(value: (ChannelId, CreateMessage)) -> Self {
        Self::ToChannel(value.0, value.1)
    }
}

impl From<(Message, CreateMessage)> for BroadcastData {
    fn from(value: (Message, CreateMessage)) -> Self {
        Self::Reply(value.0, value.1)
    }
}

impl From<(Message, EditMessage)> for BroadcastData {
    fn from(value: (Message, EditMessage)) -> Self {
        Self::Edit(value.0, value.1)
    }
}

impl From<(Message, ReactionType)> for BroadcastData {
    fn from(value: (Message, ReactionType)) -> Self {
        Self::Reaction(value.0, value.1)
    }
}

pub struct MessageBroadcaster {
    send: broadcast::Sender<Broadcast>,
}

impl MessageBroadcaster {
    pub fn new() -> Self {
        Self {
            send: broadcast::channel::<Broadcast>(60).0,
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

    pub fn send<T>(msg: T)
    where
        T: Into<Broadcast>,
    {
        Self::get().send_message(msg);
    }

    pub fn send_message<T>(&self, msg: T)
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
