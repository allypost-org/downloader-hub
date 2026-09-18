use std::sync::Arc;

use app_database::{
    Database, StreamExt,
    api::log_settings::{LogSettingsScope, resolve_log_settings},
};
use tracing::{trace, warn};

use super::ComponentResult;

pub async fn run(db: Arc<Database>) -> ComponentResult {
    trace!("Starting admin log-settings watcher");
    let mut settings = db.log_settings_watch().await?;
    while let Some(emission) = settings.next().await {
        match emission {
            Ok(rows) => {
                let effective = resolve_log_settings(&rows, LogSettingsScope::Admin);
                let settings = app_logger::LogFilterSettings {
                    console: effective.console,
                    file: effective.file,
                };
                if let Err(e) = app_logger::apply_log_filter_settings(&settings) {
                    warn!(
                        ?e,
                        "Invalid dynamic log settings; retaining current filters"
                    );
                }
            }
            Err(e) => warn!(?e, "Error reading log settings from database"),
        }
    }
    Ok(())
}
