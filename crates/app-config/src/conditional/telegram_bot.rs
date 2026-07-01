use std::path::PathBuf;

use clap::{Args, ValueHint};
use serde::{Deserialize, Serialize};
use size::Size;
use url::Url;
use validator::Validate;

use crate::validators::directory::{
    validate_is_writable_directory, value_parser_parse_valid_directory,
};

pub const OFFICIAL_API_URL: &str = "https://api.telegram.org";

#[derive(derive_more::Debug, Clone, Serialize, Deserialize, Args, Validate)]
#[clap(next_help_heading = "Telegram bot options")]
pub struct TelegramBotConfig {
    /// The telegram bot token.
    ///
    /// See API docs for more info: <https://core.telegram.org/bots/features#botfather>
    #[arg(long = "telegram-bot-token", value_name = "BOT_TOKEN", env = "DOWNLOADER_HUB_TELEGRAM_BOT_TOKEN", value_hint = ValueHint::Other)]
    pub bot_token: String,

    /// The Telegram user ID of the owner of the bot.
    ///
    /// Used to restrict access to the bot or allow additional commands
    /// By default, also saves media sent by the owner to the memes directory
    #[arg(long = "telegram-owner-id", value_name = "OWNER_ID", env = "DOWNLOADER_HUB_TELEGRAM_OWNER_ID", value_hint = ValueHint::Other)]
    pub owner_id: Option<u64>,

    /// The Telegram API URL for the bot to use.
    ///
    /// Can be used if a Local API server is in use <https://github.com/tdlib/telegram-bot-api>.
    #[arg(long = "telegram-api-url", default_value = OFFICIAL_API_URL, value_name = "API_URL", env = "DOWNLOADER_HUB_TELEGRAM_API_URL", value_hint = ValueHint::Url)]
    #[debug("{:?}", api_url.as_str())]
    pub api_url: Url,

    /// The directory to save media sent by the owner of the bot.
    ///
    /// If not set, the media will not be saved.
    /// If set, the media will be saved in the specified directory.
    /// Directory will be created if it does not exist.
    /// If the specified path isn't a writable directory, the bot will throw an error.
    #[arg(long = "telegram-owner-download-dir", value_name = "DOWNLOAD_DIR", env = "DOWNLOADER_HUB_TELEGRAM_OWNER_DOWNLOAD_DIR", value_hint = ValueHint::DirPath, value_parser = value_parser_parse_valid_directory())]
    #[validate(custom(function = "validate_is_writable_directory"))]
    pub owner_download_dir: Option<PathBuf>,

    /// The about command text for the bot.
    ///
    /// If left empty, a generic default text will be used.
    #[arg(long = "telegram-about", value_name = "ABOUT", env = "DOWNLOADER_HUB_TELEGRAM_ABOUT", value_hint = ValueHint::Other)]
    pub about: Option<String>,

    /// The maximum size of a payload to send to the Telegram API.
    ///
    /// If not set, the default value will be used.
    #[arg(long = "telegram-max-payload-size", value_name = "MAX_PAYLOAD_SIZE", env = "DOWNLOADER_HUB_TELEGRAM_MAX_PAYLOAD_SIZE", value_hint = ValueHint::Other, default_value = "50MB")]
    pub max_payload_size: Size,
}
impl TelegramBotConfig {
    #[must_use]
    pub fn is_api_url_local(&self) -> bool {
        self.api_url.as_str() != OFFICIAL_API_URL
    }

    #[must_use]
    pub fn owner_link(&self) -> Option<String> {
        self.owner_id.map(|id| format!("tg://user?id={}", id))
    }
}
