use std::path::PathBuf;

use clap::{Args, ValueHint};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::validators::directory::{
    validate_is_writable_directory, value_parser_parse_valid_directory,
};

#[derive(derive_more::Debug, Clone, Serialize, Deserialize, Args, Validate)]
#[clap(next_help_heading = "Discord bot options")]
pub struct DiscordBotConfig {
    /// The discord bot token.
    ///
    /// See API docs for more info: <https://discord.com/developers/docs/intro>
    #[arg(long = "discord-bot-token", value_name = "BOT_TOKEN", env = "DOWNLOADER_HUB_DISCORD_BOT_TOKEN", value_hint = ValueHint::Other)]
    pub bot_token: String,

    /// The Discord user ID of the owner of the bot.
    ///
    /// Used to restrict access to the bot or allow additional commands
    /// By default, also saves media sent by the owner to the memes directory
    #[arg(long = "discord-owner-id", value_name = "OWNER_ID", env = "DOWNLOADER_HUB_DISCORD_OWNER_ID", value_hint = ValueHint::Other)]
    pub owner_id: Option<u64>,

    /// Whether to hide the owner in the about command.
    #[arg(
        long = "discord-hide-owner-in-about",
        env = "DOWNLOADER_HUB_DISCORD_HIDE_OWNER_IN_ABOUT",
        default_value = "false"
    )]
    pub hide_owner_in_about: bool,

    /// The directory to save media sent by the owner of the bot.
    ///
    /// If not set, the media will not be saved.
    /// If set, the media will be saved in the specified directory.
    /// Directory will be created if it does not exist.
    /// If the specified path isn't a writable directory, the bot will throw an error.
    #[arg(long = "discord-owner-download-dir", value_name = "DOWNLOAD_DIR", env = "DOWNLOADER_HUB_DISCORD_OWNER_DOWNLOAD_DIR", value_hint = ValueHint::DirPath, value_parser = value_parser_parse_valid_directory())]
    #[validate(custom(function = "validate_is_writable_directory"))]
    pub owner_download_dir: Option<PathBuf>,

    /// The about command text for the bot.
    ///
    /// If left empty, a generic default text will be used.
    #[arg(long = "discord-about", value_name = "ABOUT", env = "DOWNLOADER_HUB_DISCORD_ABOUT", value_hint = ValueHint::Other)]
    pub about: Option<String>,
}
