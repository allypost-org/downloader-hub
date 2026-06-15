use serde::{Deserialize, Serialize};

use super::file::{FileReference, FileReferenceError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestInfo {
    DownloadAndFix(FileReference),
}

impl TryFrom<app_database::entity::requests::request_info::RequestInfo> for RequestInfo {
    type Error = RequestInfoError;

    fn try_from(
        value: app_database::entity::requests::request_info::RequestInfo,
    ) -> Result<Self, Self::Error> {
        match value {
            app_database::entity::requests::request_info::RequestInfo::DownloadAndFix(file) => {
                Ok(Self::DownloadAndFix(file.try_into()?))
            }
        }
    }
}

impl From<RequestInfo> for app_database::entity::requests::request_info::RequestInfo {
    fn from(value: RequestInfo) -> Self {
        match value {
            RequestInfo::DownloadAndFix(file) => Self::DownloadAndFix(file.into()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RequestInfoError {
    #[error("Failed to convert file reference: {0}")]
    FileReferenceError(#[from] FileReferenceError),
}
