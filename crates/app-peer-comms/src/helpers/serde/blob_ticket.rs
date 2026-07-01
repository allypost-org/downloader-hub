use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serializer};

pub fn serialize<S>(
    ticket: &iroh_blobs::ticket::BlobTicket,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(ticket.to_string().as_str())
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<iroh_blobs::ticket::BlobTicket, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;

    let s = String::deserialize(deserializer)?;

    iroh_blobs::ticket::BlobTicket::from_str(s.as_str()).map_err(D::Error::custom)
}
