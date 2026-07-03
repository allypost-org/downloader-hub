use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};

pub mod info;
pub mod status;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkRequest {
    pub request_id: Arc<str>,
    pub info: info::WorkRequestInfo,
    pub metadata: HashMap<String, String>,
    pub status: status::WorkRequestStatus,
    #[serde(default)]
    pub errors: Arc<[Arc<str>]>,
}

impl WorkRequest {
    pub fn from_db_vec(
        vec: &[app_database::api::requests::RequestInfoResponse],
    ) -> Result<Arc<[Self]>, WorkRequestError> {
        vec.iter().map(std::convert::TryInto::try_into).collect()
    }
}

impl TryFrom<app_database::api::requests::RequestInfoResponse> for WorkRequest {
    type Error = WorkRequestError;

    fn try_from(
        value: app_database::api::requests::RequestInfoResponse,
    ) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl<'a> TryFrom<&'a app_database::api::requests::RequestInfoResponse> for WorkRequest {
    type Error = WorkRequestError;

    fn try_from(
        value: &'a app_database::api::requests::RequestInfoResponse,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            request_id: value.request_id.clone(),
            info: value.info.clone().try_into()?,
            metadata: value.metadata.clone(),
            status: value.status.clone().try_into()?,
            errors: value.errors.clone(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkRequestError {
    #[error("Invalid work request info: {0}")]
    InvalidWorkRequestInfo(#[from] info::WorkRequestInfoError),

    #[error("Invalid work request status: {0}")]
    InvalidWorkRequestStatus(#[from] status::WorkRequestStatusError),
}
