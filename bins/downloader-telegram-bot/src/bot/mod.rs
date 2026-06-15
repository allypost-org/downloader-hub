pub mod helpers;

use std::{collections::HashMap, string::ToString, sync::OnceLock};

use app_actions::{
    actions::{AVAILABLE_ACTIONS, ActionOptions, handlers::ActionEntry},
    downloaders::AVAILABLE_DOWNLOADERS,
    extractors::AVAILABLE_EXTRACTORS,
    fixers::{AVAILABLE_FIXERS, handlers::FixerInstance},
};
use helpers::status_message::StatusMessage;
use teloxide::{
    adaptors::trace,
    prelude::*,
    requests::RequesterExt,
    types::{LinkPreviewOptions, ParseMode, ReplyParameters},
    utils::command::BotCommands,
};
use tracing::{Instrument, Span, field, info, trace};

use crate::{
    config::Config,
    queue::{Task, TaskQueue},
};

pub type TeloxideBot =
    teloxide::adaptors::CacheMe<trace::Trace<teloxide::adaptors::DefaultParseMode<teloxide::Bot>>>;

static TELEGRAM_BOT: OnceLock<TeloxideBot> = OnceLock::new();

pub struct TelegramBot;
impl TelegramBot {
    pub fn instance() -> &'static TeloxideBot {
        TELEGRAM_BOT.get_or_init(|| {
            let tg_config = Config::bot();

            teloxide::Bot::new(&tg_config.bot_token)
                .set_api_url(tg_config.api_url.clone())
                .parse_mode(ParseMode::Html)
                .trace(trace::Settings::TRACE_EVERYTHING)
                .cache_me()
        })
    }

    pub fn pure_instance() -> &'static teloxide::Bot {
        Self::instance().inner().inner().inner()
    }
}

#[derive(BotCommands, Debug, Clone)]
#[command(
    rename_rule = "snake_case",
    description = "These commands are supported:"
)]
enum BotCommand {
    #[command(description = "Display this text.")]
    Help,
    #[command(description = "Start using the bot.")]
    Start,
    #[command(description = "Print some info about the bot.")]
    About,
    #[command(description = "List available extractors.")]
    ListExtractors,
    #[command(description = "List available downloaders.")]
    ListDownloaders,
    #[command(description = "List available fixers.")]
    ListFixers,
    #[command(description = "List available actions.")]
    ListActions,
    #[command(description = "Responds with 'Pong!'")]
    Ping,
    #[command(
        description = "Run the specified fixers on the message.",
        parse_with = parse_fixers,
    )]
    Fix(Vec<FixerInstance>),
    #[command(
        description = "Run the specified action on the message.",
        parse_with = parse_action,
    )]
    Act(ActionEntry, ActionOptions),
}

struct CmdActParams(ActionEntry, ActionOptions);
#[allow(clippy::unnecessary_wraps)]
#[allow(clippy::needless_pass_by_value)]
fn parse_action(s: String) -> Result<CmdActParams, teloxide::utils::command::ParseError> {
    let (name, opts) = s.split_once(' ').unwrap_or((s.as_str(), ""));
    let name = name.trim();

    AVAILABLE_ACTIONS
        .iter()
        .find(|x| x.name() == name)
        .map(|x| {
            let opts = opts
                .trim()
                .split(' ')
                .filter_map(parse_option_string)
                .collect::<ActionOptions>();

            trace!(?opts, "Parsed action options");

            CmdActParams(x.clone(), opts)
        })
        .ok_or_else(|| {
            teloxide::utils::command::ParseError::IncorrectFormat(
                anyhow::anyhow!("Unknown action. Use /list_actions to see the available actions.")
                    .into(),
            )
        })
}

struct CmdFixParams(Vec<FixerInstance>);
#[allow(clippy::unnecessary_wraps)]
#[allow(clippy::needless_pass_by_value)]
fn parse_fixers(s: String) -> Result<CmdFixParams, teloxide::utils::command::ParseError> {
    let name_to_instance = AVAILABLE_FIXERS
        .iter()
        .map(|x| (x.name(), x.clone()))
        .collect::<HashMap<_, _>>();

    let res = s
        .split(' ')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(|x| x.split_once('=').unwrap_or((x, "")))
        .filter_map(|(name, _params)| name_to_instance.get(name))
        .cloned()
        .collect();

    Ok(CmdFixParams(res))
}

pub async fn run() -> anyhow::Result<()> {
    info!("Starting command bot...");

    let bot = TelegramBot::instance();
    let me = bot.get_me().await?;

    bot.set_my_commands(BotCommand::bot_commands())
        .send()
        .await
        .expect("Failed to set commands");

    info!(api_url = ?TelegramBot::pure_instance().api_url().as_str(), id = ?me.id, user = ?me.username(), name = ?me.full_name(), "Bot started");

    Box::pin(
        Dispatcher::builder(bot, Update::filter_message().endpoint(answer))
            .build()
            .dispatch(),
    )
    .await;

    Ok(())
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
                Ok(c) => handle_command(msg, c).await,
                Err(_) => handle_message(msg).await,
            }
        }
        .instrument(Span::current()),
    );

    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn handle_command(msg: Message, command: BotCommand) -> ResponseResult<()> {
    info!(?command, "Handling command");
    match command {
        BotCommand::Help => {
            TelegramBot::instance()
                .send_message(msg.chat.id, BotCommand::descriptions().to_string())
                .reply_parameters(ReplyParameters::new(msg.id).allow_sending_without_reply())
                .await?;
        }
        BotCommand::Start => {
            TelegramBot::instance()
                .send_message(
                    msg.chat.id,
                    "Hello! I'm a bot that can help download your memes.\n\nJust send me a link \
                     to a funny video and I'll do the rest!\nYou can also just send or forward a \
                     message with media and links to me and I'll fix it up for you!\n\nIf you'd \
                     like to know more use the /help or /about commands.",
                )
                .reply_parameters(ReplyParameters::new(msg.id).allow_sending_without_reply())
                .await?;
        }
        BotCommand::About => {
            let tg_config = Config::bot();

            let text = tg_config.about.clone().unwrap_or_else(|| {
                let mut paragraphs = vec![
                    r#"This bot is a part of the <a href="https://github.com/Allypost/downloader-hub/">Downloader Hub project</a>. It's a bot that helps you download your memes"#.to_string(),
                    "It is powered by Rust, yt-dlp, ffmpeg, and some external services.".to_string(),
                    "The source code is available at\nhttps://github.com/Allypost/downloader-hub/tree/main/crates/downloader-telegram-bot"
                        .to_string(),
                    "You can find out about the available downloaders and fixers, and what they do by using the /list_extractors, /list_downloaders and /list_fixers commands."
                    .to_string(),
                    "No data about downloading/users is stored outside of logs that live in RAM".to_string(),
                ];

                if let Some(owner_link) = tg_config.owner_link() {
                    paragraphs.push(format!(
                        r#"This bot instance is ran by <a href="{link}">this user</a>."#,
                        link = owner_link,
                    ));
                }

                paragraphs.join("\n\n")
            });

            trace!(?text, "Sending about message");

            TelegramBot::instance()
                .send_message(msg.chat.id, text.trim())
                .reply_parameters(ReplyParameters::new(msg.id).allow_sending_without_reply())
                .link_preview_options(LinkPreviewOptions {
                    is_disabled: true,
                    prefer_large_media: false,
                    prefer_small_media: false,
                    show_above_text: false,
                    url: None,
                })
                .await?;
        }
        BotCommand::ListExtractors => {
            let extractors_text = AVAILABLE_EXTRACTORS
                .iter()
                .map(|x| {
                    format!(
                        "<blockquote><u>{}</u>\n{}</blockquote>",
                        x.name(),
                        x.description()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");

            TelegramBot::instance()
                .send_message(
                    msg.chat.id,
                    format!(
                        "Extractors are used to get info about links. That info can then be \
                         passed to the downloaders who actually download the \
                         content.\n\nAvailable extractors:\n{}",
                        extractors_text
                    ),
                )
                .reply_parameters(ReplyParameters::new(msg.id).allow_sending_without_reply())
                .await?;
        }
        BotCommand::ListDownloaders => {
            let downloaders_text = AVAILABLE_DOWNLOADERS
                .iter()
                .map(|x| {
                    format!(
                        "<blockquote><u>{}</u>\n{}</blockquote>",
                        x.name(),
                        x.description()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");

            TelegramBot::instance()
                .send_message(
                    msg.chat.id,
                    format!(
                        "Downloaders are the things that actually download the content. They need \
                         to be given the exact info about what and how to download, which they \
                         get from extractors.\n\nAvailable downloaders:\n{}",
                        downloaders_text
                    ),
                )
                .reply_parameters(ReplyParameters::new(msg.id).allow_sending_without_reply())
                .await?;
        }
        BotCommand::ListFixers => {
            let fixers_text = AVAILABLE_FIXERS
                .iter()
                .map(|x| {
                    format!(
                        "<blockquote>{star}<u>{name}</u>\n{desc}</blockquote>",
                        name = x.name(),
                        desc = x.description(),
                        star = if x.is_enabled() { "" } else { "*" }
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n\n* Not enabled by default";

            TelegramBot::instance()
                .send_message(
                    msg.chat.id,
                    format!(
                        "Fixers are used to fix up the content somewhere on disk in various \
                         ways.\n\nAvailable fixers:\n{}",
                        fixers_text
                    ),
                )
                .reply_parameters(ReplyParameters::new(msg.id).allow_sending_without_reply())
                .await?;
        }
        BotCommand::ListActions => {
            let actions_text = AVAILABLE_ACTIONS
                .iter()
                .map(|x| {
                    format!(
                        "<blockquote><u>{}</u>\n{}</blockquote>",
                        x.name(),
                        x.description()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");

            TelegramBot::instance()
                .send_message(
                    msg.chat.id,
                    format!(
                        "Actions are used to do something with the content.\n\nAvailable \
                         actions:\n{}",
                        actions_text
                    ),
                )
                .reply_parameters(ReplyParameters::new(msg.id).allow_sending_without_reply())
                .await?;
        }
        BotCommand::Ping => {
            TelegramBot::instance()
                .send_message(msg.chat.id, "Pong!")
                .reply_parameters(ReplyParameters::new(msg.id).allow_sending_without_reply())
                .await?;
        }
        BotCommand::Fix(fixers) => {
            info!(?fixers, "Adding fix request to queue");

            let mut status_message = StatusMessage::from_message(&msg);

            status_message
                .update_message("Message queued. Waiting for spot in line...")
                .await?;

            TaskQueue::push(Task::fix_request(msg, fixers, status_message));
        }
        BotCommand::Act(action, options) => {
            info!(?action, ?options, "Adding action request to queue");

            let mut status_message = StatusMessage::from_message(&msg);

            status_message
                .update_message("Message queued. Waiting for spot in line...")
                .await?;

            TaskQueue::push(Task::action_request(msg, action, options, status_message));
        }
    }

    Ok(())
}

async fn handle_message(msg: Message) -> ResponseResult<()> {
    info!("Adding download request to queue");

    let mut status_message = StatusMessage::from_message(&msg);

    status_message
        .update_message("Message queued. Waiting for spot in line...")
        .await?;

    TaskQueue::push(Task::download_request(msg, status_message));

    Ok(())
}

fn parse_option_string(s: &str) -> Option<(String, serde_json::Value)> {
    let (k, v) = s.split_once('=').unwrap_or((s, ""));
    let (k, v) = (k.trim(), v.trim());

    if k.is_empty() {
        return None;
    }

    let v = v.replace('+', " ");

    let (k, v) = {
        let key = k.into();

        match v.as_str() {
            "" | "true" | "TRUE" => (key, true.into()),
            "false" | "FALSE" => (key, false.into()),
            _ if v.parse::<f64>().is_ok() => (key, v.parse::<f64>().unwrap_or_default().into()),
            _ => (key, v.into()),
        }
    };

    Some((k, v))
}
