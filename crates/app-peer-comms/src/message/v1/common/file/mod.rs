use std::{str::FromStr, sync::Arc};

pub use app_database::entity::requests::file_reference::FileUrl;
use app_database::entity::requests::file_reference::{
    BlobTicket as DbBlobTicket, FileReference as DbFileReference,
};
use iroh_blobs::ticket::BlobTicket as IrohBlobTicket;
use serde::{Deserialize, Serialize};

#[derive(derive_more::Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileReference {
    Url(FileUrl),
    BlobTicket(BlobTicket),
}

impl FileReference {
    pub fn url<T>(url: T) -> Self
    where
        T: Into<FileUrl>,
    {
        Self::Url(url.into())
    }

    pub fn blob_ticket<T>(ticket: T) -> Self
    where
        T: Into<BlobTicket>,
    {
        Self::BlobTicket(ticket.into())
    }
}

impl FromStr for FileReference {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

impl std::fmt::Display for FileReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_string(self).map_err(|_| std::fmt::Error)?;
        f.write_str(&s)
    }
}

impl TryFrom<DbFileReference> for FileReference {
    type Error = FileReferenceError;

    fn try_from(value: DbFileReference) -> Result<Self, Self::Error> {
        match value {
            DbFileReference::Url(url) => Ok(Self::Url(url)),
            DbFileReference::BlobTicket(ticket) => Ok(Self::BlobTicket(ticket.try_into()?)),
        }
    }
}

impl From<FileReference> for DbFileReference {
    fn from(value: FileReference) -> Self {
        match value {
            FileReference::Url(url) => Self::Url(url),
            FileReference::BlobTicket(ticket) => Self::BlobTicket(ticket.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobTicket {
    #[serde(with = "crate::helpers::serde::blob_ticket")]
    pub ticket: IrohBlobTicket,
    pub file_name: Arc<str>,
}

impl<T> From<(IrohBlobTicket, T)> for BlobTicket
where
    T: Into<Arc<str>>,
{
    fn from(value: (IrohBlobTicket, T)) -> Self {
        Self {
            ticket: value.0,
            file_name: value.1.into(),
        }
    }
}

impl TryFrom<DbBlobTicket> for BlobTicket {
    type Error = FileReferenceError;

    fn try_from(value: DbBlobTicket) -> Result<Self, Self::Error> {
        Ok(Self {
            ticket: value.ticket.parse()?,
            file_name: value.file_name,
        })
    }
}

impl From<BlobTicket> for DbBlobTicket {
    fn from(value: BlobTicket) -> Self {
        Self {
            ticket: value.ticket.to_string(),
            file_name: value.file_name,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FileReferenceError {
    #[error("Invalid blob ticket: {0}")]
    InvalidBlobTicket(#[from] iroh_tickets::ParseError),
}
