use app_config::conditional::discord_bot::DiscordBotConfig;
use clap::Args;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
pub struct DiscordConfig {
    #[clap(flatten)]
    #[validate(nested)]
    pub bot: DiscordBotConfig,
}
