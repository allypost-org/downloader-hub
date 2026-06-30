use std::sync::{Arc, OnceLock};

use app_config::{common::Size, conditional::discord_bot::DiscordBotConfig};
use serenity::{http::Http, model::id::UserId};
use tokio::sync::{AcquireError, OwnedSemaphorePermit, Semaphore};

pub struct DiscordBot {
    http: Arc<Http>,
    config: Arc<DiscordBotConfig>,
    work_request_sem: Arc<Semaphore>,
}

static DISCORD_BOT: OnceLock<DiscordBot> = OnceLock::new();

impl DiscordBot {
    pub fn init(http: Arc<Http>, config: Arc<DiscordBotConfig>) {
        let work_request_sem = Arc::new(Semaphore::new(config.max_work_request_concurrency));
        _ = DISCORD_BOT.set(Self {
            http,
            config,
            work_request_sem,
        });
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

    pub async fn acquire_work_request_permit() -> Result<OwnedSemaphorePermit, AcquireError> {
        Self::instance()
            .work_request_sem
            .clone()
            .acquire_owned()
            .await
    }
}
