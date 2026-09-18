use clap::Args;
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::entry_id::EntryId;

#[derive(Debug, Clone, Default, Serialize, Deserialize, Args, Validate)]
#[clap(next_help_heading = Some("Action entries"))]
pub struct DisabledEntriesConfig {
    #[clap(
        long = "disable-action-entry",
        value_delimiter = ',',
        env = "DOWNLOADER_HUB_DISABLED_ACTION_ENTRIES"
    )]
    pub entries: Vec<EntryId>,
}
