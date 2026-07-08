use std::sync::{Arc, OnceLock};

use app_config::{common::Size, conditional::discord_bot::DiscordBotConfig};
use serenity::{http::Http, model::id::UserId};

pub struct DiscordBot {
    http: Arc<Http>,
    config: Arc<DiscordBotConfig>,
}

static DISCORD_BOT: OnceLock<DiscordBot> = OnceLock::new();

impl DiscordBot {
    pub fn init(http: Arc<Http>, config: Arc<DiscordBotConfig>) {
        _ = DISCORD_BOT.set(Self { http, config });
    }

    pub fn instance() -> &'static Self {
        DISCORD_BOT.get().expect("Discord bot not initialized")
    }

    pub fn bot() -> &'static Arc<Http> {
        &Self::instance().http
    }

    #[must_use]
    pub fn owner_id() -> Option<UserId> {
        Self::instance().config.owner_id.map(UserId::new)
    }

    pub fn owner_download_dir() -> Option<std::path::PathBuf> {
        Self::instance().config.owner_download_dir.clone()
    }

    pub fn max_payload_size() -> Size {
        Self::instance().config.max_payload_size
    }

    pub fn max_payload_bytes() -> u64 {
        u64::try_from(Self::max_payload_size().bytes())
            .expect("max payload size must not be negative")
    }
}
