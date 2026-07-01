use app_config::conditional::telegram_bot::TelegramBotConfig;
use clap::Args;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize, Args, Validate)]
pub struct TelegramConfig {
    #[clap(flatten)]
    #[validate(nested)]
    pub bot: TelegramBotConfig,
}
