use serde::{Deserialize, Serialize};

use crate::message::v1::common::file::{FileReference, FileReferenceError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkRequestInfo {
    DownloadAndFix(FileReference),
    RefreshAccountInfo(app_database::entity::requests::request_info::RefreshAccountInfoPayload),
}

impl TryFrom<app_database::entity::requests::request_info::RequestInfo> for WorkRequestInfo {
    type Error = WorkRequestInfoError;

    fn try_from(
        value: app_database::entity::requests::request_info::RequestInfo,
    ) -> Result<Self, Self::Error> {
        match value {
            app_database::entity::requests::request_info::RequestInfo::DownloadAndFix(
                file_reference,
            ) => file_reference
                .try_into()
                .map(Self::DownloadAndFix)
                .map_err(Into::into),
            app_database::entity::requests::request_info::RequestInfo::RefreshAccountInfo(payload) => {
                Ok(Self::RefreshAccountInfo(payload))
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkRequestInfoError {
    #[error("Invalid file reference: {0}")]
    InvalidFileReference(#[from] FileReferenceError),
}
