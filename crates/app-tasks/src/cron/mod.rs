use tracing::{Instrument, Span, debug, error, info, info_span};

use crate::config::TaskConfig;

pub mod tasks;

#[tracing::instrument(name = "cron", skip_all)]
pub fn spawn() {
    info!("Spawning cron tasks");

    let span = info_span!("tasks");
    let _span = span.enter();
    if let Some(yt_dlp_update_interval) = TaskConfig::global().yt_dlp_update_interval {
        debug!(interval = ?yt_dlp_update_interval, "Spawning yt-dlp update task");
        tokio::task::spawn(
            async move {
                loop {
                    tokio::time::sleep(yt_dlp_update_interval.into()).await;

                    if let Err(e) = tasks::yt_dlp::update_yt_dlp().await {
                        error!("Failed to update yt-dlp: {e:?}");
                    }

                    info!("Updated yt-dlp");
                }
            }
            .instrument(Span::current()),
        );
    }
}
