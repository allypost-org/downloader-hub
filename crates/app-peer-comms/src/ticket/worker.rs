use std::{fmt::Display, str::FromStr};

use super::{Ticket, TicketError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkerTicket(Ticket);

impl WorkerTicket {
    #[must_use]
    pub const fn new(ticket: Ticket) -> Self {
        Self(ticket)
    }

    #[must_use]
    pub const fn ticket(&self) -> &Ticket {
        &self.0
    }
}

impl From<Ticket> for WorkerTicket {
    fn from(value: Ticket) -> Self {
        Self(value)
    }
}

impl From<WorkerTicket> for Ticket {
    fn from(value: WorkerTicket) -> Self {
        value.0
    }
}

impl Display for WorkerTicket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "wrkr{}", self.0)
    }
}

impl FromStr for WorkerTicket {
    type Err = TicketError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.strip_prefix("wrkr").ok_or(TicketError::InvalidFormat)?;

        s.parse().map(Self)
    }
}
