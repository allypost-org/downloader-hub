use clap::Subcommand;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", tag = "$cmd")]
pub enum CmdConfig {
    /// Run as a telegram bot
    Telegram(Box<super::telegram::config::TelegramConfig>),
    /// Run as a discord bot
    Discord(Box<super::discord::config::DiscordConfig>),
}

impl Validate for CmdConfig {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        match self {
            Self::Telegram(cfg) => cfg.validate(),
            Self::Discord(cfg) => cfg.validate(),
        }
    }
}
