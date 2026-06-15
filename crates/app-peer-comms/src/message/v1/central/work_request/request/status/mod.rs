use app_database::entity::requests::file_reference::FileReference as DbFileReference;
use serde::{Deserialize, Serialize};

use crate::message::v1::common::file::{FileReference, FileReferenceError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkRequestStatus {
    Pending,
    InProgress(ProgressInfo),
    Done { at: u64, by: String },
    Failed { at: u64, by: String, reason: String },
}

impl WorkRequestStatus {
    #[must_use]
    pub const fn is_pending(&self) -> bool {
        matches!(self, Self::Pending)
    }

    #[must_use]
    pub const fn is_in_progress(&self) -> bool {
        matches!(self, Self::InProgress { .. })
    }

    #[must_use]
    pub fn progress_info(&self) -> Option<ProgressInfo> {
        match self {
            Self::InProgress(x) => Some(x.clone()),
            _ => None,
        }
    }

    #[must_use]
    pub const fn is_done(&self) -> bool {
        matches!(self, Self::Done { .. })
    }

    #[must_use]
    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    #[must_use]
    pub fn failed_reason(&self) -> Option<&str> {
        match self {
            Self::Failed { reason, .. } => Some(reason),
            _ => None,
        }
    }

    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.is_done() || self.is_failed()
    }
}

impl TryFrom<app_database::api::requests::RequestStatus> for WorkRequestStatus {
    type Error = WorkRequestStatusError;

    fn try_from(value: app_database::api::requests::RequestStatus) -> Result<Self, Self::Error> {
        match value {
            app_database::api::requests::RequestStatus::Pending => Ok(Self::Pending),
            app_database::api::requests::RequestStatus::InProgress {
                since,
                by,
                message,
                waiting_for_requester,
                files_data,
            } => Ok(Self::InProgress(ProgressInfo {
                since,
                by,
                message,
                waiting_for_requester: waiting_for_requester.unwrap_or(false),
                files_data: {
                    let files_data = files_data.unwrap_or_else(|| "[]".to_string());
                    let db_files_data: Vec<DbFileReference> = serde_json::from_str(&files_data)
                        .map_err(WorkRequestStatusError::InvalidWorkRequestStatus)?;

                    Some(
                        db_files_data
                            .into_iter()
                            .map(|db| {
                                db.try_into()
                                    .map_err(WorkRequestStatusError::InvalidFileReference)
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                    )
                },
            })),
            app_database::api::requests::RequestStatus::Done { at, by } => {
                Ok(Self::Done { at, by })
            }
            app_database::api::requests::RequestStatus::Failed { at, by, reason } => {
                Ok(Self::Failed { at, by, reason })
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkRequestStatusError {
    #[error("Invalid work request status: {0}")]
    InvalidWorkRequestStatus(serde_json::Error),

    #[error("Invalid file reference: {0}")]
    InvalidFileReference(#[from] FileReferenceError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressInfo {
    pub since: u64,
    pub by: String,
    pub message: Option<String>,
    pub waiting_for_requester: bool,
    pub files_data: Option<Vec<FileReference>>,
}
