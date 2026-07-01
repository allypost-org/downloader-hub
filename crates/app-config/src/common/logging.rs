use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, ValueEnum, PartialEq, Eq)]
pub enum LogFormat {
    #[default]
    Pretty,
    Plain,
    Json,
}
