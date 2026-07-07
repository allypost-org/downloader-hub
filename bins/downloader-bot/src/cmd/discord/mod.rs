use std::sync::Arc;

use serenity::{Client, all::GatewayIntents};
use tracing::{debug, error, info, instrument};

use super::CmdResult;
use crate::cmd::discord::{bot::discord_bot::DiscordBot, config::DiscordConfig};

mod bot;
pub mod broadcaster;
pub mod config;

#[instrument(name = "discord", skip_all)]
pub async fn run(config: DiscordConfig) -> CmdResult {
    _ = broadcaster::MessageBroadcaster::init();

    info!("Starting discord bot...");

    let bot_config = Arc::new(config.bot.clone());

    let intents = GatewayIntents::non_privileged()
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::GUILD_MESSAGE_REACTIONS
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::DIRECT_MESSAGE_REACTIONS
        | GatewayIntents::MESSAGE_CONTENT;

    debug!(?intents, "Discord bot intents");

    let mut client = Client::builder(config.bot.bot_token, intents)
        .event_handler(bot::Handler::new(bot::HandlerConfig {
            about_text: config.bot.about.clone().unwrap_or_else(|| {
                let mut paragraphs = vec![
                    "This bot is a part of the [Downloader Hub project](https://github.com/allynet/downloader-hub/). It's a bot that helps you download your memes".to_string(),
                    "It is powered by Rust, yt-dlp, ffmpeg, and some external services.".to_string(),
                    "The source code is available [on GitHub](https://github.com/allynet/downloader-hub/tree/main/bins/downloader-bot)"
                        .to_string(),
                    "You can find out about the available extractors, downloaders and fixers by using the /list_extractors, /list_downloaders and /list_fixers commands."
                        .to_string(),
                    "No data about downloading/users is stored outside of logs that live in RAM".to_string(),
                ];

                if !config.bot.hide_owner_in_about && let Some(owner_id) = config.bot.owner_id {
                    paragraphs.push(format!(
                        "This bot instance is ran by <@{}>.",
                        owner_id,
                    ));
                }

                paragraphs.join("\n\n")
            }),
        }))
        .await?;

    DiscordBot::init(client.http.clone(), bot_config);

    if let Err(e) = client.start_autosharded().await {
        error!(?e, "Failed to start discord bot");
        return Err(e.into());
    }

    info!("Discord bot exited gracefully");

    Ok(())
}
