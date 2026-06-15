use std::{ffi::OsStr, path::PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::fs;
use tracing::{debug, trace};

use crate::fixers::{
    Fixer, FixerReturn, IntoFixerReturn,
    common::{FixRequest, FixResult, FixerError},
};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct FileExtension;

#[async_trait::async_trait]
#[typetag::serde]
impl Fixer for FileExtension {
    fn description(&self) -> &'static str {
        "Fix file extensions to match the file type."
    }

    /// Options:
    ///
    async fn run(&self, request: &FixRequest) -> FixerReturn {
        fix_file_extension(request.clone()).await
    }
}

async fn fix_file_extension(request: FixRequest) -> FixerReturn {
    let file_path = request.file_path.clone();
    debug!("Checking file extension for {file_path:?}...");

    let extension = file_path.extension().and_then(OsStr::to_str);

    let file_ext = {
        let file_path = file_path.clone();

        tokio::task::spawn_blocking(move || infer::get_from_path(file_path)).await?
    };
    let file_ext = match file_ext {
        Ok(Some(ext)) => ext.extension(),
        _ => {
            return FileExtensionError::UnableToGetExtension(file_path.clone()).into_fixer_return();
        }
    };
    debug!("Inferred file extension: {:?}", file_ext);

    if let Some(extension) = extension
        && extension == file_ext
    {
        debug!("File extension is correct");
        return Ok(FixResult::new(request, file_path.clone()));
    }

    trace!(
        "File extension is incorrect ({:?} vs ({:?}))",
        extension, file_ext
    );

    let new_file_path = file_path.with_extension(file_ext);

    debug!("Renaming file from {file_path:?} to {new_file_path:?}");
    match fs::rename(file_path, &new_file_path).await {
        Ok(()) => Ok(FixResult::new(request, new_file_path.clone())),
        Err(e) => FileExtensionError::UnableToRenameFile(e).into_fixer_return(),
    }
}

#[derive(Debug, Error)]
pub enum FileExtensionError {
    #[error("Unable to get extension of {0:?}")]
    UnableToGetExtension(PathBuf),
    #[error("Unable to rename file: {0:?}")]
    UnableToRenameFile(std::io::Error),
}

impl IntoFixerReturn for FileExtensionError {
    fn into_fixer_return(self) -> FixerReturn {
        Err(FixerError::FailedFix(self.into()))
    }
}
