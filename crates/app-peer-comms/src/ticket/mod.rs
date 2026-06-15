use std::{fmt, str::FromStr, sync::Arc};

use iroh::EndpointAddr;
use iroh_gossip::proto::TopicId;
use serde::{Deserialize, Serialize};
use url::Url;

pub mod targeted;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticket {
    pub topic: TopicId,
    pub main: EndpointAddr,
    pub peers: Arc<[EndpointAddr]>,
    pub refresh_url: Option<Url>,
    pub refresh_token: Option<Arc<str>>,
}
impl Ticket {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, TicketError> {
        postcard::from_bytes(bytes).map_err(TicketError::PostcardDecode)
    }
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        postcard::to_stdvec(self).expect("postcard::to_stdvec is infallible")
    }

    #[must_use]
    pub const fn topic_id(&self) -> TopicId {
        self.topic
    }

    #[must_use]
    pub fn peers(&self) -> Arc<[EndpointAddr]> {
        self.peers.clone()
    }
}

impl Ticket {
    #[must_use]
    pub const fn encoding() -> data_encoding::Encoding {
        data_encoding::BASE32_NOPAD_NOCASE
    }
}

impl fmt::Display for Ticket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut text = Self::encoding().encode(self.to_bytes().as_ref());
        text.make_ascii_lowercase();
        write!(f, "dlhub{text}")
    }
}

impl FromStr for Ticket {
    type Err = TicketError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.strip_prefix("dlhub").ok_or(TicketError::InvalidFormat)?;

        let bytes = Self::encoding()
            .decode(s.as_bytes())
            .map_err(TicketError::Decode)?;

        Self::from_bytes(&bytes)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TicketError {
    #[error("Invalid ticket format")]
    InvalidFormat,

    #[error("Failed to decode ticket from string: {0}")]
    Decode(#[from] data_encoding::DecodeError),

    #[error("Failed to decode data from ticket: {0}")]
    PostcardDecode(postcard::Error),
}
