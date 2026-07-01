use std::path::PathBuf;

use app_config::{
    GlobalConfig,
    common::{ProgramPathConfig, ProjectConfig},
};
use validator::Validate;

#[derive(Debug, Clone, Default, Validate, GlobalConfig)]
pub(crate) struct HelpersConfig {
    /// Path to various programs used by the application at runtime
    #[validate(nested)]
    pub dependency_paths: ProgramPathConfig,
}

impl HelpersConfig {
    #[must_use]
    #[inline]
    pub fn cache_dir() -> PathBuf {
        ProjectConfig::cache_dir()
    }

    #[must_use]
    #[inline]
    pub fn dependency_paths() -> &'static ProgramPathConfig {
        &Self::global().dependency_paths
    }
}

pub fn init(dependency_paths: ProgramPathConfig) -> Result<(), String> {
    HelpersConfig::init(HelpersConfig { dependency_paths })?;

    Ok(())
}
