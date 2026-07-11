use serde::{Deserialize, Serialize};

use super::file::{FileReference, FileReferenceError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RequestInfo {
    DownloadAndFix(FileReference),
    RefreshAccountInfo(app_database::entity::requests::request_info::RefreshAccountInfoPayload),
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
            app_database::entity::requests::request_info::RequestInfo::RefreshAccountInfo(payload) => {
                Ok(Self::RefreshAccountInfo(payload))
            }
        }
    }
}

impl From<RequestInfo> for app_database::entity::requests::request_info::RequestInfo {
    fn from(value: RequestInfo) -> Self {
        match value {
            RequestInfo::DownloadAndFix(file) => Self::DownloadAndFix(file.into()),
            RequestInfo::RefreshAccountInfo(payload) => Self::RefreshAccountInfo(payload),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RequestInfoError {
    #[error("Failed to convert file reference: {0}")]
    FileReferenceError(#[from] FileReferenceError),
}
