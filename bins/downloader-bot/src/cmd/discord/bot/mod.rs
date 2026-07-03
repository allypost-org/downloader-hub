use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use clap::Parser;
use serde::{Deserialize, Serialize};
use serenity::{
    all::{CreateMessage, GuildId, Message as SerenityMessage, Ready, ResumedEvent},
    prelude::*,
};
use tracing::{debug, info, trace, warn};

use crate::{
    cmd::discord::broadcaster::{Broadcast, BroadcastData, MessageBroadcaster},
    config::Config,
};

pub mod discord_bot;
pub mod handlers;
pub mod helpers;

pub struct Handler {
    is_loop_running: AtomicBool,
    about_text: String,
}

pub struct HandlerConfig {
    pub about_text: String,
}

impl Handler {
    pub fn new(config: HandlerConfig) -> Self {
        Self {
            is_loop_running: AtomicBool::new(false),
            about_text: config.about_text,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
#[command(
    multicall = true,
    disable_help_flag = true,
    disable_version_flag = true
)]
pub struct BotCommandWrapper {
    #[command(subcommand)]
    pub command: BotCommand,
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub enum BotCommand {
    /// Ping the bot to check if it is alive.
    /// Should always just respond with "Pong!"
    Ping,

    /// Print the about message for the bot.
    About,

    /// Download and fix the given URLs.
    /// You can specify multiple URLs by separating them with a space.
    /// The bot will download all supported files from the URLs and fix them up using the available fixers.
    /// The resulting files will be sent as a reply to the message that triggered the command.
    #[clap(visible_aliases = ["df"])]
    DownloadAndFix { urls: Vec<url::Url> },

    /// List the available extractors (URL handlers).
    ListExtractors,

    /// List the available downloaders.
    ListDownloaders,

    /// List the available fixers (post-processors).
    ListFixers,
}
impl BotCommand {
    pub fn parse(s: &str) -> Result<Self, clap::Error> {
        let Some(args) = shlex::split(s) else {
            return Err(clap::Error::new(
                clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand,
            ));
        };

        let matches = BotCommandWrapper::try_parse_from(args)?;

        Ok(matches.command)
    }
}

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!(id = %ready.user.id, user = ?ready.user.name, shard = ?ready.shard, "Discord bot ready");

        trace!("Setting activity");
        ctx.shard
            .set_activity(Some(serenity::all::ActivityData::custom(
                Config::app_name_with_version(),
            )));
        trace!("Activity set");
    }

    async fn cache_ready(&self, ctx: Context, _guilds: Vec<GuildId>) {
        debug!(?_guilds, "Cache ready");

        if self.is_loop_running.load(Ordering::Relaxed) {
            return;
        }

        let ctx = Arc::new(ctx);

        tokio::task::spawn(async move {
            loop {
                if let Err(e) = handlers::work_request::watch_work_requests().await {
                    warn!(?e, "Work requests watcher exited with error");
                    if let Err(re) = crate::peering::reconnect().await {
                        warn!(?re, "Failed to re-bootstrap irpc session; will retry");
                    }
                }

                let about_two_seconds = 2000 + rand::random_range(0..=2000);
                let about_two_seconds = std::time::Duration::from_millis(about_two_seconds);

                debug!(time = ?about_two_seconds, "Work requests stream finished. Sleeping for a random amount of time...");

                tokio::time::sleep(about_two_seconds).await;
            }
        });

        tokio::task::spawn({
            async move {
                loop {
                    debug!("Starting to watch message broadcaster");
                    let mut broadcast_iter = MessageBroadcaster::get().recv();
                    while let Ok(broadcast) = broadcast_iter.recv().await {
                        let Err(broadcast_err) =
                            handle_broadcast(broadcast.clone(), ctx.clone()).await
                        else {
                            continue;
                        };

                        if !matches!(
                            broadcast_err,
                            serenity::Error::Io(_) | serenity::Error::ExceededLimit(_, _)
                        ) {
                            continue;
                        }

                        warn!(?broadcast, ?broadcast_err, "Failed to handle broadcast");
                        tokio::task::spawn(async move {
                            let delay = 1000 + rand::random_range(0..=1000);
                            let delay = std::time::Duration::from_millis(delay);
                            debug!(?broadcast, ?delay, "Repeating broadcast after delay");
                            tokio::time::sleep(delay).await;
                            trace!(?broadcast, "Sending broadcast");
                            MessageBroadcaster::send(broadcast);
                        });
                    }

                    let about_two_seconds = 2000 + rand::random_range(0..=2000);
                    let about_two_seconds = std::time::Duration::from_millis(about_two_seconds);

                    debug!(time = ?about_two_seconds, "Message broadcaster stream finished. Sleeping for a random amount of time...");

                    tokio::time::sleep(about_two_seconds).await;
                }
            }
        });

        self.is_loop_running.store(true, Ordering::Relaxed);
    }

    async fn resume(&self, _ctx: Context, _: ResumedEvent) {
        info!("Discord bot resumed");
    }

    async fn message(&self, ctx: Context, msg: SerenityMessage) {
        if msg.author.bot {
            trace!("Message is from a bot");
            return;
        }

        let bot_id = ctx.cache.current_user().id;

        if msg.author.id == bot_id {
            return;
        }

        if msg.guild_id.is_some() && !msg.mentions_me(&ctx).await.is_ok_and(|x| x) {
            trace!("Message is not a private message and does not mention bot");
            return;
        }

        let clean_content = msg.content.replace(&format!("<@{}>", bot_id), "");
        let clean_content = clean_content.trim();
        let clean_content = if let Some(c) = clean_content.chars().next() {
            if c.is_alphanumeric() {
                format!("/{}", clean_content)
            } else {
                format!("/{}", &clean_content[1..])
            }
        } else {
            trace!("Message is empty");
            return;
        };

        let urls = handlers::message::urls_in_message(&msg);

        let cmd = match BotCommand::parse(&clean_content) {
            Ok(x) => x,
            Err(e) => {
                if !urls.is_empty() {
                    trace!("Parse failed but message has URLs, treating as free-form download");
                    handlers::message::handle_download_request(&msg, urls).await;
                    return;
                }

                MessageBroadcaster::send(Broadcast::from_data((
                    msg.channel_id,
                    CreateMessage::new().content(format!("```\n{}\n```", e)),
                )));
                return;
            }
        };

        match cmd {
            BotCommand::Ping => {
                _ = msg.reply(&ctx, "Pong!").await;
            }
            BotCommand::About => {
                _ = msg.reply(&ctx, self.about_text.clone()).await;
            }
            BotCommand::DownloadAndFix { urls: cmd_urls } => {
                let combined_urls = if cmd_urls.is_empty() { urls } else { cmd_urls };
                handlers::message::handle_download_request(&msg, combined_urls).await;
            }
            BotCommand::ListExtractors | BotCommand::ListDownloaders | BotCommand::ListFixers => {
                use crate::cmd::_common::capabilities::{CapabilityKind, fetch, render};
                let kind = match cmd {
                    BotCommand::ListExtractors => CapabilityKind::Extractors,
                    BotCommand::ListDownloaders => CapabilityKind::Downloaders,
                    _ => CapabilityKind::Fixers,
                };
                let text = fetch().await.map_or_else(
                    || "Failed to fetch capabilities from central.".to_string(),
                    |summary| render(kind, &summary),
                );
                _ = msg.reply(&ctx, text).await;
            }
        }
    }
}

async fn handle_broadcast(broadcast: Broadcast, ctx: Arc<Context>) -> Result<(), serenity::Error> {
    match broadcast.data.as_ref() {
        BroadcastData::Global(msg) => {
            trace!(?msg, "Global message");
        }
        BroadcastData::ToChannel(channel_id, msg) => {
            trace!(?channel_id, ?msg, "To channel message");
            channel_id.send_message(&ctx, msg.clone()).await?;
        }
        BroadcastData::Reply(msg, reply_msg) => {
            trace!(?msg, ?reply_msg, "Reply message");
            let reply_msg = reply_msg.clone().reference_message(msg);
            msg.channel_id.send_message(&ctx, reply_msg).await?;
        }
        BroadcastData::Edit(msg, edit_msg) => {
            trace!(?msg, ?edit_msg, "Edit message");
            let mut msg = msg.clone();
            msg.edit(&ctx, edit_msg.clone()).await?;
        }
        BroadcastData::Reaction(msg, reaction) => {
            trace!(?msg, ?reaction, "Reaction message");
            msg.react(&ctx, reaction.clone()).await?;
        }
    }

    Ok(())
}
