use serde::{Deserialize, Serialize};

pub use super::Message;

pub mod bot;
pub mod central;
pub mod common;
pub mod worker;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum V1Message {
    Worker(worker::WorkerMessage),
    Central(central::CentralMessage),
    Bot(bot::BotMessage),
}

impl From<V1Message> for Message {
    fn from(msg: V1Message) -> Self {
        Self::V1(msg)
    }
}
