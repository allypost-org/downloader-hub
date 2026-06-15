pub use app_macros::{Dumpable, GlobalConfig};
pub use traits::*;

pub mod common;
pub mod conditional;
pub mod timeframe;
pub mod traits;
pub mod validators;

pub const BUILD_DATE: &str = compile_time::datetime_str!();
pub const BUILD_RUSTC_VERSION: &str = compile_time::rustc_version_str!();
