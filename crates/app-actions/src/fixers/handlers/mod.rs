pub mod crop_image;
pub mod crop_video_bars;
pub mod file_extensions;
pub mod file_name;
pub mod media_formats;

use std::sync::{Arc, LazyLock};

use crate::fixers::Fixer;

pub type FixerInstance = Arc<dyn Fixer + Send + Sync>;

pub static ALL_FIXERS: LazyLock<Vec<FixerInstance>> = LazyLock::new(all_fixers);
pub static AVAILABLE_FIXERS: LazyLock<Vec<FixerInstance>> = LazyLock::new(available_fixers);

fn all_fixers() -> Vec<FixerInstance> {
    vec![
        Arc::new(file_extensions::FileExtension),
        Arc::new(file_name::FileName),
        Arc::new(media_formats::MediaFormats),
        Arc::new(crop_video_bars::CropVideoBars),
        Arc::new(crop_image::CropImage),
    ]
}

fn available_fixers() -> Vec<FixerInstance> {
    all_fixers()
        .into_iter()
        .filter(|f| f.can_run() && f.is_enabled())
        .collect()
}
