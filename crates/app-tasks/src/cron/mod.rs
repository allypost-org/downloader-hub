use jiff::tz::TimeZone;
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
                    let relative = jiff::Timestamp::now().to_zoned(TimeZone::UTC);
                    let interval = yt_dlp_update_interval
                        .to_duration(&relative)
                        .expect("yt-dlp update interval must resolve to a concrete duration");
                    let interval = std::time::Duration::try_from(interval.abs())
                        .expect("yt-dlp update interval must fit in std::time::Duration");
                    tokio::time::sleep(interval).await;

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
