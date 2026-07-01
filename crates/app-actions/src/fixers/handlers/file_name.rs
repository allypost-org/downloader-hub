use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::fs;
use tracing::{debug, trace};

use crate::fixers::{
    Fixer, FixerReturn, IntoFixerReturn,
    common::{FixRequest, FixResult, FixerError},
};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct FileName;

#[async_trait::async_trait]
#[typetag::serde]
impl Fixer for FileName {
    fn description(&self) -> &'static str {
        "Fix file name to contain only approved characters."
    }

    /// Options:
    ///
    async fn run(&self, request: &FixRequest) -> FixerReturn {
        fix_file_name(request.clone()).await
    }
}

async fn fix_file_name(request: FixRequest) -> FixerReturn {
    let file_path = request.file_path.clone();
    debug!("Checking file name for {file_path:?}...");
    let name = file_path.file_stem().and_then(|x| x.to_str());

    let new_name = match name {
        Some(name) if !name.is_ascii() => {
            debug!("File name {name:?} contains non-ascii characters. Trying to fix...");
            name.replace(|c: char| !c.is_ascii(), "")
        }
        None => {
            return FileNameError::NoName(file_path.clone()).into_fixer_return();
        }
        Some(name) => {
            debug!("File name for {name:?} is OK. Skipping...");
            return Ok(FixResult::new(request, file_path.clone()));
        }
    };

    let extension = file_path
        .extension()
        .and_then(|x| x.to_str())
        .ok_or_else(|| FileNameError::NoExtension(file_path.clone()))
        .map_err(FixerError::failed_fix)?;

    trace!("New file name: {new_name:?} (extension: {extension:?}) for file {file_path:?}");

    let new_name = format!("{new_name}.{extension}");
    let new_file_path = file_path.with_file_name(new_name);

    debug!("Renaming file from {file_path:?} to {new_file_path:?}");

    fs::rename(file_path, &new_file_path)
        .await
        .map(|()| FixResult::new(request, new_file_path.clone()))
        .map_err(FileNameError::Rename)
        .map_err(FixerError::failed_fix)
}

#[derive(Debug, Error)]
pub enum FileNameError {
    #[error("Failed to get name for file {0:?}")]
    NoName(PathBuf),
    #[error("Failed to get extension for file {0:?}")]
    NoExtension(PathBuf),
    #[error("Failed to rename file: {0:?}")]
    Rename(std::io::Error),
}

impl IntoFixerReturn for FileNameError {
    fn into_fixer_return(self) -> FixerReturn {
        Err(FixerError::failed_fix(self))
    }
}
