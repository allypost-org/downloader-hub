use serde::{Deserialize, Serialize};

use super::Ticket;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TargetedTicket(Ticket);

impl TargetedTicket {
    #[must_use]
    pub const fn new(ticket: Ticket) -> Self {
        Self(ticket)
    }

    #[must_use]
    pub const fn ticket(&self) -> &Ticket {
        &self.0
    }

    #[must_use]
    pub fn to_string<T>(&self, target: T) -> String
    where
        T: Into<TicketTarget>,
    {
        format!("{}{}", target.into().short(), self.0)
    }

    pub fn from_str<T>(s: &str, target: T) -> Result<Self, TargetedTicketError>
    where
        T: Into<TicketTarget>,
    {
        let s = s
            .strip_prefix(target.into().short())
            .ok_or(TargetedTicketError::InvalidFormat)?;

        s.parse().map(Self).map_err(TargetedTicketError::Decode)
    }
}

impl From<Ticket> for TargetedTicket {
    fn from(value: Ticket) -> Self {
        Self(value)
    }
}

impl From<TargetedTicket> for Ticket {
    fn from(value: TargetedTicket) -> Self {
        value.0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TargetedTicketError {
    #[error("Invalid ticket format")]
    InvalidFormat,

    #[error("Failed to decode ticket from string: {0}")]
    Decode(#[from] super::TicketError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TicketTarget {
    #[serde(rename = "bot")]
    Bot,
    #[serde(rename = "worker")]
    Worker,
    #[serde(rename = "admin")]
    Admin,
}

impl TicketTarget {
    #[must_use]
    pub const fn short(&self) -> &'static str {
        match self {
            Self::Bot => "bot",
            Self::Worker => "wrkr",
            Self::Admin => "admn",
        }
    }
}

impl std::fmt::Display for TicketTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        serde_json::to_value(self)
            .expect("Failed to serialize TicketTarget")
            .as_str()
            .expect("Failed to get string from TicketTarget")
            .fmt(f)
    }
}

impl std::str::FromStr for TicketTarget {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}
