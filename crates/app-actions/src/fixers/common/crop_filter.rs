use std::{fmt::Display, path::PathBuf, process::Stdio};

use app_helpers::ffprobe;
use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
};
use tracing::{debug, trace};

use super::{FixerError, FixerReturn, command::CmdError};
use crate::{config::ActionsConfig, fixers::IntoFixerReturn};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CropFilter {
    pub width: i64,
    pub height: i64,
    pub x: i64,
    pub y: i64,
}

impl CropFilter {
    pub const fn new_min() -> Self {
        Self {
            width: 0,
            height: 0,
            x: i64::MAX,
            y: i64::MAX,
        }
    }

    pub fn union(&mut self, other: &Self) {
        self.width = self.width.max(other.width);
        self.height = self.height.max(other.height);
        self.x = self.x.min(other.x);
        self.y = self.y.min(other.y);
    }

    pub fn intersect(&mut self, other: &Self) {
        self.width = self.width.min(other.width);
        self.height = self.height.min(other.height);
        self.x = self.x.max(other.x);
        self.y = self.y.max(other.y);
    }

    pub fn to_imagemagick_dimensions(&self) -> String {
        let mut x = self.x.to_string();
        if self.x >= 0 {
            x = format!("+{}", x);
        }

        let mut y = self.y.to_string();
        if self.y >= 0 {
            y = format!("+{}", y);
        }

        format!("{}x{}{}{}", self.width, self.height, x, y)
    }

    pub async fn from_image_files(files: &[PathBuf]) -> Result<Self, CropError> {
        generate_crop_filter_for_files(files, None).await
    }
}

impl Display for CropFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "crop={width}:{height}:{x}:{y}",
            width = self.width,
            height = self.height,
            x = self.x,
            y = self.y
        )
    }
}

#[derive(Debug, Error)]
pub enum CropError {
    #[error("Invalid path: {0:?}")]
    InvalidPath(PathBuf, String),
    #[error(transparent)]
    FfProbeError(#[from] ffprobe::FfProbeError),
    #[error(transparent)]
    CommandError(#[from] CmdError),
    #[error("Failed to get width and height of media")]
    NoDimensions(PathBuf),
    #[error("Failed to get file {0} from {1:?}")]
    NoFilePart(String, PathBuf),
    #[error("No filters found for {0:?}")]
    NoFilters(PathBuf),
    #[error("Failed to create temp dir")]
    TempDirError(std::io::Error),
}

impl From<CropError> for FixerError {
    fn from(val: CropError) -> Self {
        Self::FailedFix(val.into())
    }
}

impl IntoFixerReturn for CropError {
    fn into_fixer_return(self) -> FixerReturn {
        Err(FixerError::FailedFix(self.into()))
    }
}

async fn generate_crop_filter_for_files(
    files: &[PathBuf],
    initial_filter: Option<&CropFilter>,
) -> Result<CropFilter, CropError> {
    const TRIM_PASSES: u8 = 2;
    const FUZZ_PERCENTAGE: u8 = 15;
    const SHAVE_BORDER_PIXELS: u8 = 2;
    const MIN_WIDTH: i64 = 4;
    const MIN_HEIGHT: i64 = 4;
    let mut cmd = Command::new(
        ActionsConfig::dependency_paths()
            .imagemagick_path()
            .expect("Imagemagick not found"),
    );
    let res = {
        let mut res = cmd.args(files);
        if let Some(filter) = initial_filter {
            res = res.arg("-crop").arg(filter.to_imagemagick_dimensions());
        } else {
            // Remove small outside border to encourage better trimmming
            res = res.args([
                "-shave".to_string(),
                format!("{px}x{px}", px = SHAVE_BORDER_PIXELS),
            ]);
        }
        for _ in 0..TRIM_PASSES {
            res = res
                .args(["-fuzz".to_string(), format!("{}%", FUZZ_PERCENTAGE)])
                .arg("-trim");
        }
        res.args(["-format", "%w:%h:%X:%Y\n"]).arg("info:-")
    };
    debug!(
        cmd = ?res.as_std(),
        "Running command to generate crop filters"
    );
    let mut res = res
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| CropError::CommandError(CmdError::Run(e)))?;

    let mut res_stdout_lines = {
        let stdout = res.stdout.take().expect("stdout should be available");
        let reader = BufReader::new(stdout);
        reader.lines()
    };

    let filter = {
        let parse_line = |line: &str| {
            let mut s = line.split(':');
            let mut next_s = || {
                s.next()
                    .and_then(|x| x.trim_start_matches('+').to_string().parse::<i64>().ok())
            };

            Some(CropFilter {
                width: next_s()?,
                height: next_s()?,
                x: next_s()?,
                y: next_s()?,
            })
        };

        trace!("Reading crop filter command output lines");
        let mut filter = CropFilter::new_min();
        while let Ok(Some(line)) = res_stdout_lines.next_line().await {
            let Some(line_filter) = parse_line(&line) else {
                trace!(?line, "Couldn't parse line, skipping");
                continue;
            };

            if line_filter.width < MIN_WIDTH || line_filter.height < MIN_HEIGHT {
                trace!(?line, ?line_filter, "Filter is too small, skipping");
                continue;
            }

            if line_filter.x < 0 || line_filter.y < 0 {
                trace!(?line, ?line_filter, "Filter has negative offset, skipping");
                continue;
            }

            // trace!(?line, ?line_filter, "Parsed line to filter");
            filter.union(&line_filter);
        }

        debug!(?filter, "Generated filter from output");

        filter
    };

    let res = res
        .wait()
        .await
        .map_err(|e| CropError::CommandError(CmdError::Run(e)))?;

    if res.success() {
        Ok(filter)
    } else {
        Err(CropError::CommandError(CmdError::FailedStatus(
            format!("Command exited with non-zero exit code, {:?}", res),
            res,
        )))
    }
}
