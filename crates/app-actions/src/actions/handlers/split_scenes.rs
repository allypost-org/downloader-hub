use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use app_helpers::temp_dir::TempDir;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{fs, process::Command};

use super::{Action, ActionError, ActionRequest, ActionResult};
use crate::config::ActionsConfig;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SplitScenes;

#[async_trait::async_trait]
#[typetag::serde]
impl Action for SplitScenes {
    fn description(&self) -> &'static str {
        "Detect scenes in a video and split them into separate files."
    }

    async fn can_run(&self) -> bool {
        ActionsConfig::dependency_paths()
            .scenedetect_path()
            .is_some()
    }

    async fn can_run_for(&self, _req: &ActionRequest) -> bool {
        true
    }

    /// Options:
    /// - `output-dir`: The output directory. Defaults to a temporary directory.
    async fn run(&self, request: &ActionRequest) -> Result<ActionResult, ActionError> {
        do_split_video_into_scenes(&request.file_path, &request.output_dir)
            .await
            .map(|x| ActionResult::paths(request, x))
    }
}

async fn do_split_video_into_scenes(
    file_path: &Path,
    output_dir: &Path,
) -> Result<Vec<PathBuf>, ActionError> {
    split_into_scenes(SplitVideoConfig::new(
        output_dir.to_path_buf(),
        file_path.to_path_buf(),
    ))
    .await
}

#[derive(Debug, Clone)]
pub struct SplitVideoConfig {
    pub download_dir: PathBuf,
    pub file_path: PathBuf,
    pub file_template: Option<String>,
}

impl SplitVideoConfig {
    #[must_use]
    pub const fn new(download_dir: PathBuf, file_path: PathBuf) -> Self {
        Self {
            download_dir,
            file_path,
            file_template: None,
        }
    }

    #[must_use]
    pub fn with_file_template(&mut self, file_template: String) -> &Self {
        self.file_template = Some(file_template);
        self
    }
}

async fn split_into_scenes(config: SplitVideoConfig) -> Result<Vec<PathBuf>, ActionError> {
    let scenedetect_path = match ActionsConfig::dependency_paths().scenedetect_path() {
        Some(x) => x,
        None => return Err(SplitScenesError::ScenedetectNotFound.into()),
    };

    let temp_dir = TempDir::in_tmp_with_prefix("split-scenes")
        .map_err(|x| SplitScenesError::TempDirCreate(x.into()))?;

    let mut cmd = Command::new(scenedetect_path);
    let cmd = cmd
        .args(["--input", config.file_path.to_str().unwrap_or_default()])
        .arg("detect-adaptive")
        .arg("split-video")
        .arg("--high-quality")
        .args(["--preset", "medium"])
        .arg("--output")
        .arg(temp_dir.path())
        .args([
            "--filename",
            &config
                .file_template
                .unwrap_or_else(|| "$VIDEO_NAME.$SCENE_NUMBER".to_string()),
        ])
        .stdin(Stdio::null());

    let output = cmd
        .output()
        .await
        .map_err(SplitScenesError::ScenedetectRun)?;

    if !output.status.success() {
        return Err(SplitScenesError::ScenedetectExited(output.status.code()).into());
    }

    let scenes = {
        let mut iter = fs::read_dir(temp_dir.path())
            .await
            .map_err(SplitScenesError::TempDirRead)?;

        let mut scenes = Vec::new();
        while let Ok(Some(entry)) = iter.next_entry().await {
            let path_in_temp_dir = entry.path();

            let file_name = match path_in_temp_dir.file_name() {
                Some(x) => x,
                None => continue,
            };

            let path_in_download_dir = config.download_dir.join(file_name);

            if fs::rename(path_in_temp_dir, &path_in_download_dir)
                .await
                .is_ok()
            {
                scenes.push(path_in_download_dir);
            }
        }

        scenes
    };

    Ok(scenes)
}

#[derive(Debug, Error)]
pub enum SplitScenesError {
    #[error("Error while creating temp dir: {0:?}")]
    TempDirCreate(anyhow::Error),
    #[error("Error while reading temp dir: {0:?}")]
    TempDirRead(std::io::Error),
    #[error("Scenedetect not found")]
    ScenedetectNotFound,
    #[error("Error while running scenedetect: {0}")]
    ScenedetectRun(std::io::Error),
    #[error("Scenedetect exited with error code {0:?}")]
    ScenedetectExited(Option<i32>),
}

impl From<SplitScenesError> for ActionError {
    fn from(val: SplitScenesError) -> Self {
        Self::FailedAction(val.into())
    }
}
