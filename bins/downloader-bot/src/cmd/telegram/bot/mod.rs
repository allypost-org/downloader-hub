pub mod helpers;

use std::{
    ops::Deref,
    string::ToString,
    sync::{Arc, OnceLock},
};

use app_config::{common::Size, conditional::telegram_bot::TelegramBotConfig};
use teloxide::{
    adaptors::trace, prelude::*, requests::RequesterExt, types::ParseMode,
    utils::command::BotCommands,
};
use tracing::{Instrument, Span, field, info, trace};

pub mod handlers;

pub type TeloxideBot =
    teloxide::adaptors::CacheMe<trace::Trace<teloxide::adaptors::DefaultParseMode<teloxide::Bot>>>;

static TELEGRAM_BOT: OnceLock<TelegramBot> = OnceLock::new();

pub struct TelegramBot {
    inner: TeloxideBot,
    config: Arc<TelegramBotConfig>,
}

impl Deref for TelegramBot {
    type Target = TeloxideBot;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl TelegramBot {
    pub fn new(config: TelegramBotConfig) -> Self {
        let bot = teloxide::Bot::new(&config.bot_token)
            .set_api_url(config.api_url.clone())
            .parse_mode(ParseMode::Html)
            .trace(trace::Settings::TRACE_EVERYTHING)
            .cache_me();

        Self {
            inner: bot,
            config: Arc::new(config),
        }
    }

    pub fn init(config: TelegramBotConfig) {
        _ = TELEGRAM_BOT.set(Self::new(config));
    }

    pub fn instance() -> &'static Self {
        TELEGRAM_BOT.get().expect("Telegram bot not initialized")
    }

    pub fn bot() -> &'static teloxide::Bot {
        Self::instance().inner().inner().inner()
    }

    #[inline]
    pub fn max_payload_size() -> Size {
        Self::instance().config.max_payload_size
    }

    #[must_use]
    pub fn owner_id() -> Option<teloxide::types::UserId> {
        Self::instance()
            .config
            .owner_id
            .map(teloxide::types::UserId)
    }

    pub fn owner_download_dir() -> Option<std::path::PathBuf> {
        Self::instance().config.owner_download_dir.clone()
    }
}

impl TelegramBot {
    pub async fn run() -> anyhow::Result<()> {
        info!("Starting command bot...");

        let bot = Self::instance();
        let me = bot.get_me().await?;

        bot.set_my_commands(BotCommand::bot_commands())
            .send()
            .await
            .expect("Failed to set commands");

        info!(api_url = ?Self::bot().api_url().as_str(), id = ?me.id, user = ?me.username(), name = ?me.full_name(), "Bot started");

        Box::pin(
            Dispatcher::builder(&bot.inner, Update::filter_message().endpoint(answer))
                .build()
                .dispatch(),
        )
        .await;

        Ok(())
    }
}

#[derive(BotCommands, Debug, Clone)]
#[command(
    rename_rule = "snake_case",
    description = "These commands are supported:"
)]
pub enum BotCommand {
    #[command(description = "Display this text.")]
    Help,
    #[command(description = "Start using the bot.")]
    Start,
    #[command(description = "Print some info about the bot.")]
    About,
    #[command(description = "Responds with 'Pong!'")]
    Ping,
}

#[tracing::instrument(name = "message", skip(_bot, msg), fields(chat = %msg.chat.id, msg_id = %msg.id, with = field::Empty))]
async fn answer(_bot: &TeloxideBot, msg: Message) -> ResponseResult<()> {
    trace!(?msg, "Got message");

    tokio::task::spawn(
        async move {
            {
                let name = msg
                    .chat
                    .username()
                    .map(|x| format!("@{}", x))
                    .or_else(|| msg.chat.title().map(ToString::to_string))
                    .or_else(|| {
                        let mut name = String::new();
                        if let Some(first_name) = msg.chat.first_name() {
                            name.push_str(first_name);
                        }
                        if let Some(last_name) = msg.chat.last_name() {
                            name.push(' ');
                            name.push_str(last_name);
                        }

                        Some(name)
                    });

                if let Some(name) = name {
                    Span::current().record("with", field::debug(name));
                }
            }

            let bot_me = TelegramBot::instance().get_me().await?;

            let msg_text = msg
                .text()
                .or_else(|| msg.caption())
                .map(ToString::to_string)
                .unwrap_or_default();

            match BotCommand::parse(&msg_text, bot_me.username()) {
                Ok(c) => handlers::command::handle_command(&msg, c).await,
                Err(_) => handlers::message::handle_message(&msg).await,
            }
        }
        .in_current_span(),
    );

    Ok(())
}
