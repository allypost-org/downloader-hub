use std::{env, path::PathBuf};

use directories_next::ProjectDirs;

pub static APPLICATION_NAME: &str = match option_env!("APPLICATION_NAME") {
    Some(name) => name,
    None => "downloader-hub",
};
pub static ORGANIZATION_NAME: &str = "allypost";
pub static ORGANIZATION_QUALIFIER: &str = "net";

pub struct ProjectConfig;
impl ProjectConfig {
    #[must_use]
    #[inline]
    pub fn config_dir() -> Option<PathBuf> {
        Self::get_project_dir().map(|x| x.config_dir().into())
    }

    #[must_use]
    #[inline]
    pub fn get_config_dir(&self) -> Option<PathBuf> {
        Self::config_dir()
    }

    #[must_use]
    #[inline]
    pub fn cache_dir() -> PathBuf {
        Self::get_project_dir().map_or_else(
            || env::temp_dir().join(APPLICATION_NAME),
            |x| x.cache_dir().into(),
        )
    }

    #[must_use]
    #[inline]
    pub fn get_cache_dir(&self) -> PathBuf {
        Self::cache_dir()
    }

    #[must_use]
    #[inline]
    pub fn get_project_dir() -> Option<ProjectDirs> {
        ProjectDirs::from(ORGANIZATION_QUALIFIER, ORGANIZATION_NAME, APPLICATION_NAME)
    }
}
