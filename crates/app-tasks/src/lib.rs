use tracing::error;

pub(crate) mod cron;

use app_config::common;

pub struct TaskRunner;

impl TaskRunner {
    pub async fn run(conf: common::TaskConfig) -> Self {
        if let Err(e) = tokio::task::spawn_blocking(move || cron::spawn(conf)).await {
            error!("Failed to spawn cron tasks: {e:?}");
        }

        Self
    }
}
