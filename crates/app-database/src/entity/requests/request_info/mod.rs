use serde::{Deserialize, Serialize};

use super::file_reference::FileReference;

#[derive(derive_more::Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RequestInfo {
    DownloadAndFix(FileReference),
}

impl RequestInfo {
    pub fn download_and_fix<T>(file_reference: T) -> Self
    where
        T: Into<FileReference>,
    {
        Self::DownloadAndFix(file_reference.into())
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
