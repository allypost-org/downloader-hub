use std::sync::Arc;

use app_config::{common::Size, conditional::discord_bot::DiscordBotConfig};
use serenity::{all::PremiumTier, http::Http, model::id::UserId};

const DEFAULT_MAX_FILESIZE: Size = Size::from_const(10 * size::MEGABYTE);
const TIER_2_MAX_FILESIZE: Size = Size::from_const(50 * size::MEGABYTE);
const TIER_3_MAX_FILESIZE: Size = Size::from_const(100 * size::MEGABYTE);

pub struct DiscordBot {
    http: Arc<Http>,
    config: Arc<DiscordBotConfig>,
}

pub struct DiscordBotKey;

impl serenity::prelude::TypeMapKey for DiscordBotKey {
    type Value = Arc<DiscordBot>;
}

impl DiscordBot {
    pub const fn new(http: Arc<Http>, config: Arc<DiscordBotConfig>) -> Self {
        Self { http, config }
    }

    pub const fn bot(&self) -> &Arc<Http> {
        &self.http
    }

    #[must_use]
    pub fn owner_id(&self) -> Option<UserId> {
        self.config.owner_id.map(UserId::new)
    }

    #[must_use]
    pub fn owner_download_dir(&self) -> Option<std::path::PathBuf> {
        self.config.owner_download_dir.clone()
    }

    #[must_use]
    pub fn max_payload_size(&self) -> Size {
        self.config.max_payload_size
    }

    #[must_use]
    pub fn configured_max_filesize(&self) -> Size {
        self.max_payload_size()
    }

    pub const fn destination_max_filesize(premium_tier: Option<PremiumTier>) -> Size {
        match premium_tier {
            Some(PremiumTier::Tier2) => TIER_2_MAX_FILESIZE,
            Some(PremiumTier::Tier3) => TIER_3_MAX_FILESIZE,
            _ => DEFAULT_MAX_FILESIZE,
        }
    }

    #[must_use]
    pub fn safe_max_filesize(&self) -> Size {
        self.configured_max_filesize().min(DEFAULT_MAX_FILESIZE)
    }
}
