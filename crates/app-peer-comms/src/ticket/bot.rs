use std::{fmt::Display, str::FromStr};

use super::{Ticket, TicketError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BotTicket(Ticket);

impl BotTicket {
    #[must_use]
    pub const fn new(ticket: Ticket) -> Self {
        Self(ticket)
    }

    #[must_use]
    pub const fn ticket(&self) -> &Ticket {
        &self.0
    }
}

impl From<Ticket> for BotTicket {
    fn from(value: Ticket) -> Self {
        Self(value)
    }
}

impl From<BotTicket> for Ticket {
    fn from(value: BotTicket) -> Self {
        value.0
    }
}

impl Display for BotTicket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bot{}", self.0)
    }
}

impl FromStr for BotTicket {
    type Err = TicketError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.strip_prefix("bot").ok_or(TicketError::InvalidFormat)?;

        s.parse().map(Self)
    }
}
