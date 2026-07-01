use std::ops::Deref;

use app_config::{GlobalConfig, common};

#[derive(Debug, Clone, GlobalConfig)]
pub(crate) struct TaskConfig {
    pub conf: common::TaskConfig,
}

impl Deref for TaskConfig {
    type Target = common::TaskConfig;

    fn deref(&self) -> &Self::Target {
        &self.conf
    }
}

pub fn init(task: common::TaskConfig) -> Result<(), String> {
    TaskConfig::init(TaskConfig { conf: task })?;

    Ok(())
}
