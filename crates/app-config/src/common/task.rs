use clap::Args;
use jiff::Span;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Default, Serialize, Deserialize, Args, Validate)]
#[clap(next_help_heading = Some("Task options"))]
pub struct TaskConfig {
    /// The interval at which the bot should check for updates to the yt-dlp binary.
    /// If not set, the bot will not check for updates.
    ///
    /// Accepts jiff's duration syntax, e.g. `1h 3m`, `90 minutes`, `2 weeks`, `3 months`.
    /// Calendar units (days/weeks/months) follow real calendar durations relative to the
    /// check time (a month is the actual next calendar month, not a fixed 30 days).
    #[clap(short, long, env = "DOWNLOADER_HUB_YT_DLP_UPDATE_INTERVAL")]
    pub yt_dlp_update_interval: Option<Span>,
}
