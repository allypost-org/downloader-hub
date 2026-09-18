pub mod compact_media;
pub mod file_rename_to_id;
pub mod ocr_image;
pub mod remove_background;
pub mod split_scenes;

use std::sync::{Arc, LazyLock};

use super::{Action, ActionError, ActionRequest, ActionResult};

pub type ActionEntry = Arc<dyn Action>;

pub static ALL_ACTIONS: LazyLock<Vec<ActionEntry>> = LazyLock::new(all_actions);

fn all_actions() -> Vec<ActionEntry> {
    vec![
        Arc::new(file_rename_to_id::RenameToId),
        Arc::new(split_scenes::SplitScenes),
        Arc::new(compact_media::CompactMedia),
        Arc::new(ocr_image::OcrImage),
        Arc::new(remove_background::RemoveBackground),
    ]
}
