pub mod helpers;

use std::{ops::Deref, string::ToString, sync::Arc};

use app_config::{
    common::{ProgramPathConfig, Size},
    conditional::telegram_bot::TelegramBotConfig,
};
use teloxide::{
    adaptors::trace, prelude::*, requests::RequesterExt, types::ParseMode,
    utils::command::BotCommands,
};
use tracing::{Instrument, Span, field, info, trace};

pub mod handlers;

pub type TeloxideBot =
    teloxide::adaptors::CacheMe<trace::Trace<teloxide::adaptors::DefaultParseMode<teloxide::Bot>>>;

const OFFICIAL_API_MAX_FILESIZE: Size = Size::from_const(50 * size::MEGABYTE);
const LOCAL_API_MAX_FILESIZE: Size = Size::from_const(2 * size::GIGABYTE);

pub struct TelegramBot {
    inner: TeloxideBot,
    config: Arc<TelegramBotConfig>,
    dependency_paths: Arc<ProgramPathConfig>,
}

impl Deref for TelegramBot {
    type Target = TeloxideBot;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl TelegramBot {
    pub fn new(config: TelegramBotConfig, dependency_paths: Arc<ProgramPathConfig>) -> Self {
        let bot = teloxide::Bot::new(&config.bot_token)
            .set_api_url(config.api_url.clone())
            .parse_mode(ParseMode::Html)
            .trace(trace::Settings::TRACE_EVERYTHING)
            .cache_me();

        Self {
            inner: bot,
            config: Arc::new(config),
            dependency_paths,
        }
    }

    pub fn bot(&self) -> &teloxide::Bot {
        self.inner.inner().inner().inner()
    }

    #[inline]
    #[must_use]
    pub fn max_payload_size(&self) -> Size {
        self.config.max_payload_size
    }

    #[must_use]
    pub fn effective_max_filesize(&self) -> Size {
        let configured = self.max_payload_size();
        let platform = if self.config.is_api_url_local() {
            LOCAL_API_MAX_FILESIZE
        } else {
            OFFICIAL_API_MAX_FILESIZE
        };
        configured.min(platform)
    }

    #[must_use]
    pub fn owner_id(&self) -> Option<teloxide::types::UserId> {
        self.config.owner_id.map(teloxide::types::UserId)
    }

    #[must_use]
    pub fn owner_download_dir(&self) -> Option<std::path::PathBuf> {
        self.config.owner_download_dir.clone()
    }
}

impl TelegramBot {
    pub async fn run(
        bot: Arc<Self>,
        rpc: Arc<crate::peering::rpc::RpcClient>,
    ) -> anyhow::Result<()> {
        info!("Starting command bot...");

        let me = bot.get_me().await?;

        bot.set_my_commands(BotCommand::bot_commands())
            .send()
            .await
            .expect("Failed to set commands");

        info!(api_url = ?bot.bot().api_url().as_str(), id = ?me.id, user = ?me.username(), name = ?me.full_name(), "Bot started");

        let mut dispatcher =
            Dispatcher::builder(bot.inner.clone(), Update::filter_message().endpoint(answer))
                .dependencies(dptree::deps![bot, rpc])
                .build();

        Box::pin(dispatcher.dispatch()).await;

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
    #[command(description = "List the available extractors (URL handlers).")]
    ListExtractors,
    #[command(description = "List the available downloaders.")]
    ListDownloaders,
    #[command(description = "List the available fixers (post-processors).")]
    ListFixers,
}

#[tracing::instrument(name = "message", skip(tg, rpc, msg), fields(chat = %msg.chat.id, msg_id = %msg.id, with = field::Empty))]
async fn answer(
    tg: Arc<TelegramBot>,
    rpc: Arc<crate::peering::rpc::RpcClient>,
    msg: Message,
) -> ResponseResult<()> {
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

            let bot_me = tg.get_me().await?;

            let msg_text = msg
                .text()
                .or_else(|| msg.caption())
                .map(ToString::to_string)
                .unwrap_or_default();

            match BotCommand::parse(&msg_text, bot_me.username()) {
                Ok(c) => handlers::command::handle_command(&tg, &rpc, &msg, c).await,
                Err(_) => handlers::message::handle_message(&tg, &rpc, &msg).await,
            }
        }
        .in_current_span(),
    );

    Ok(())
}
