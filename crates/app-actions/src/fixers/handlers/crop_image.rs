use std::process::Stdio;

use app_helpers::{file_type, trash::move_to_trash};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tracing::{debug, warn};

use crate::{
    config::ActionsConfig,
    fixers::{
        FixRequest, FixResult, Fixer, FixerError, FixerReturn, common::crop_filter::CropFilter,
    },
};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct CropImage;

#[async_trait::async_trait]
#[typetag::serde]
impl Fixer for CropImage {
    fn can_run(&self) -> bool {
        ActionsConfig::dependency_paths()
            .imagemagick_path()
            .is_some()
    }

    fn description(&self) -> &'static str {
        "Crops dead space around the image. Any solid color surrounding the image will be cropped \
         quite aggressively."
    }

    async fn can_run_for(&self, request: &FixRequest) -> bool {
        let path = request.file_path.clone();
        tokio::task::spawn_blocking(move || file_type::infer_file_type(&path).ok())
            .await
            .ok()
            .flatten()
            .is_some_and(|x| x.type_() == file_type::mime::IMAGE)
    }

    async fn run(&self, request: &FixRequest) -> FixerReturn {
        debug!(path = ?request.file_path, "Auto cropping image");

        let input_file_path = &request.file_path;
        let output_file_path = {
            let path = request.file_path.clone();
            let mut file_name = path.file_stem().unwrap_or_default().to_os_string();
            file_name.push(".ac");
            let file_extension = path.extension().unwrap_or_default().to_os_string();
            file_name.push(".");
            file_name.push(file_extension);

            path.with_file_name(file_name)
        };

        let crop_filter =
            CropFilter::from_image_files(std::slice::from_ref(input_file_path)).await?;

        debug!(?crop_filter, "Got crop filter");

        let mut cmd = {
            let mut cmd = Command::new(
                ActionsConfig::dependency_paths()
                    .imagemagick_path()
                    .expect("Imagemagick not found"),
            );
            cmd.arg(input_file_path);
            cmd.arg("-crop")
                .arg(crop_filter.to_imagemagick_dimensions());
            cmd.arg("+repage");
            cmd.arg(&output_file_path);
            cmd.stdin(Stdio::null());
            cmd
        };

        debug!(?cmd, "Running command to crop image");

        let output = cmd
            .output()
            .await
            .map_err(|e| FixerError::CommandError(e.into()))?;

        if !output.status.success() {
            debug!(status = ?output.status, "Failed to crop image");
            return Err(FixerError::CommandError(
                anyhow::anyhow!(format!(
                    "Imagemagick returned an error: {}",
                    String::from_utf8_lossy(&output.stderr)
                ))
                .into(),
            ));
        }

        if !output_file_path.exists() {
            warn!(
                path = ?output_file_path,
                "Imagemagick did not create the output file",
            );
            return Err(FixerError::FailedFix(
                anyhow::anyhow!(format!(
                    "Imagemagick finished but the output file {path:?} does not exist",
                    path = output_file_path,
                ))
                .into(),
            ));
        }

        if let Err(e) = move_to_trash(input_file_path) {
            warn!(file = ?input_file_path, ?e, "Failed to move file to trash");
        }

        Ok(FixResult::new(request.clone(), output_file_path))
    }
}
