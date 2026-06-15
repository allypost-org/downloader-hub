pub mod compact_media;
pub mod file_rename_to_id;
pub mod ocr_image;
pub mod remove_background;
pub mod split_scenes;

use std::sync::{Arc, LazyLock};

use futures::{StreamExt, stream::FuturesUnordered};
use tracing::trace;

use super::{Action, ActionError, ActionRequest, ActionResult};

pub type ActionEntry = Arc<dyn Action>;

pub static ALL_ACTIONS: LazyLock<Vec<ActionEntry>> = LazyLock::new(all_actions);

pub static AVAILABLE_ACTIONS: LazyLock<Vec<ActionEntry>> = LazyLock::new(available_actions);

fn all_actions() -> Vec<ActionEntry> {
    vec![
        Arc::new(file_rename_to_id::RenameToId),
        Arc::new(split_scenes::SplitScenes),
        Arc::new(compact_media::CompactMedia),
        Arc::new(ocr_image::OcrImage),
        Arc::new(remove_background::RemoveBackground),
    ]
}

#[must_use]
fn available_actions() -> Vec<ActionEntry> {
    futures::executor::block_on(async move {
        all_actions()
            .into_iter()
            .map(|x| async move {
                if !x.is_enabled() {
                    trace!(?x, "Action is disabled");
                    return (x, false);
                }
                trace!(?x, "Checking if action can run");
                let can_run = x.can_run().await;
                trace!(?x, can_run, "Checked if action can run");
                (x, can_run)
            })
            .collect::<FuturesUnordered<_>>()
            .collect::<Vec<_>>()
            .await
    })
    .into_iter()
    .filter_map(|(x, y)| if y { Some(x) } else { None })
    .collect()
}
