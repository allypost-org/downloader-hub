use std::{ffi::OsStr, path::PathBuf};

use app_helpers::id::time_id;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::fs;

use super::{Action, ActionError, ActionRequest, ActionResult};
use crate::ActionCtx;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RenameToId;

#[async_trait::async_trait]
#[typetag::serde]
impl Action for RenameToId {
    fn description(&self) -> &'static str {
        "Rename file to match standard file naming convention \
         ($TIME_ID.$ORIGINAL_CROPPED_NAME.$EXT)"
    }

    async fn run(
        &self,
        _ctx: &ActionCtx,
        request: &ActionRequest,
    ) -> Result<ActionResult, ActionError> {
        rename_file_to_id(request)
            .await
            .map(|x| ActionResult::path(request, x))
    }
}

pub async fn rename_file_to_id(request: &ActionRequest) -> Result<PathBuf, ActionError> {
    let file_path = request.file_path.as_path();
    let output_dir = request.output_dir.as_path();
    let original_file_name = match file_path.file_stem().and_then(OsStr::to_str) {
        Some(file_name) => file_name,
        None => return Err(RenameToIdError::NoFileName(file_path.to_path_buf()).into()),
    };
    let original_file_ext = match file_path.extension().and_then(OsStr::to_str) {
        Some(file_ext) => file_ext,
        None => return Err(RenameToIdError::NoFileExtension(file_path.to_path_buf()).into()),
    };

    let id = time_id();
    let new_file_path = {
        let new_name = format!(
            "{id}.{name}.{ext}",
            id = id,
            name = original_file_name,
            ext = original_file_ext
        );

        output_dir.join(new_name)
    };

    fs::rename(file_path, &new_file_path)
        .await
        .map_err(RenameToIdError::Rename)?;

    Ok(new_file_path)
}

#[derive(Debug, Error)]
pub enum RenameToIdError {
    #[error("Couldn't get file name for {0:?}")]
    NoFileName(PathBuf),
    #[error("Couldn't get file extension for {0:?}")]
    NoFileExtension(PathBuf),
    #[error("Failed to rename file: {0:?}")]
    Rename(std::io::Error),
}

impl From<RenameToIdError> for ActionError {
    fn from(value: RenameToIdError) -> Self {
        Self::FailedAction(value.into())
    }
}
