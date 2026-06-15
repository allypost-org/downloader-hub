use std::{collections::HashSet, path::PathBuf};

use app_config::{
    GlobalConfig,
    common::{DisableEntry, EndpointConfig, ProgramPathConfig, ProjectConfig, RequestConfig},
};
use validator::Validate;

#[derive(Debug, Clone, Default, Validate, GlobalConfig)]
pub(crate) struct ActionsConfig {
    #[validate(nested)]
    pub endpoint: EndpointConfig,

    #[validate(nested)]
    pub dependency_paths: ProgramPathConfig,

    #[validate(nested)]
    pub request: RequestConfig,

    pub disabled_entries: HashSet<DisableEntry>,
}

impl ActionsConfig {
    #[must_use]
    #[inline]
    pub fn endpoints() -> &'static EndpointConfig {
        &Self::global().endpoint
    }

    #[must_use]
    #[inline]
    pub fn dependency_paths() -> &'static ProgramPathConfig {
        &Self::global().dependency_paths
    }

    #[must_use]
    #[inline]
    pub fn cache_dir() -> PathBuf {
        ProjectConfig::cache_dir()
    }

    #[must_use]
    #[inline]
    pub fn request() -> &'static RequestConfig {
        &Self::global().request
    }

    pub fn is_enabled<E>(&self, entry: E) -> bool
    where
        E: Into<DisableEntry>,
    {
        !self.disabled_entries.contains(&entry.into())
    }
}

pub fn init<DE, DEI>(
    endpoint: EndpointConfig,
    dependency_paths: ProgramPathConfig,
    disabled_entries: DE,
    request: RequestConfig,
) -> Result<(), String>
where
    DE: IntoIterator<Item = DEI>,
    DEI: Into<DisableEntry>,
{
    let _ = app_helpers::config::init(dependency_paths.clone());

    ActionsConfig::init(ActionsConfig {
        endpoint,
        dependency_paths,
        request,
        disabled_entries: disabled_entries
            .into_iter()
            .map(std::convert::Into::into)
            .collect(),
    })?;

    Ok(())
}
