use serde::{Deserialize, Serialize};

use crate::entity::accounts::{AccountPlaceRef, AccountUserRef};

use super::file_reference::FileReference;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkKind {
    Download,
    AccountRefresh,
}

impl WorkKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Download => "download",
            Self::AccountRefresh => "accountRefresh",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshAccountInfoPayload {
    #[serde(default)]
    pub users: Vec<AccountUserRef>,
    #[serde(default)]
    pub places: Vec<AccountPlaceRef>,
}

#[derive(derive_more::Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RequestInfo {
    DownloadAndFix(FileReference),
    RefreshAccountInfo(RefreshAccountInfoPayload),
}

impl RequestInfo {
    pub fn download_and_fix<T>(file_reference: T) -> Self
    where
        T: Into<FileReference>,
    {
        Self::DownloadAndFix(file_reference.into())
    }

    #[must_use]
    pub const fn work_kind(&self) -> WorkKind {
        match self {
            Self::DownloadAndFix(_) => WorkKind::Download,
            Self::RefreshAccountInfo(_) => WorkKind::AccountRefresh,
        }
    }
}

impl RequestInfo {
    pub fn deserialize_db<'de, D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        let str = String::deserialize(deserializer)?;

        serde_json::from_str(&str).map_err(|e| D::Error::custom(e.to_string()))
    }
}
