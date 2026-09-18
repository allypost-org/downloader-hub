use std::{fs, path::PathBuf};

use app_config::common::ProjectConfig;

use crate::id::time_thread_id;

pub fn create_temp_dir() -> anyhow::Result<PathBuf> {
    let id = time_thread_id();
    let temp_dir = ProjectConfig::cache_dir().join(id);

    fs::create_dir_all(&temp_dir)?;

    Ok(temp_dir)
}
