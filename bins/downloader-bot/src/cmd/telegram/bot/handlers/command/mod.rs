use std::sync::Arc;

use teloxide::{
    prelude::*,
    types::{LinkPreviewOptions, Message, ReplyParameters},
    utils::command::BotCommands,
};
use tracing::{info, trace};

use crate::{
    cmd::telegram::bot::{BotCommand, TelegramBot, helpers::retried::try_send_to_retrying},
    peering::rpc::RpcClient,
};

async fn send_command_message(
    tg: &Arc<TelegramBot>,
    msg: &Message,
    text: impl Into<String>,
    link_preview_options: Option<LinkPreviewOptions>,
) -> ResponseResult<Message> {
    let tg = Arc::clone(tg);
    try_send_to_retrying(
        msg.chat.id,
        (text.into(), msg.id, link_preview_options),
        Box::new(move |chat_id, (text, reply_to, link_preview_options)| {
            let tg = Arc::clone(&tg);
            async move {
                let request = tg
                    .send_message(chat_id, text)
                    .reply_parameters(ReplyParameters::new(reply_to).allow_sending_without_reply());
                if let Some(options) = link_preview_options {
                    request.link_preview_options(options).await
                } else {
                    request.await
                }
            }
        }),
    )
    .await
}

#[allow(clippy::too_many_lines)]
pub async fn handle_command(
    tg: &Arc<TelegramBot>,
    rpc: &Arc<RpcClient>,
    msg: &Message,
    command: BotCommand,
) -> ResponseResult<()> {
    info!(?command, "Handling command");
    match command {
        BotCommand::Help => {
            send_command_message(tg, msg, BotCommand::descriptions().to_string(), None).await?;
        }
        BotCommand::Start => {
            send_command_message(
                tg,
                msg,
                "Hello! I'm a bot that can help download your memes.\n\nJust send me a link to a \
                 funny video and I'll do the rest!\nYou can also just send or forward a message \
                 with media and links to me and I'll fix it up for you!\n\nIf you'd like to know \
                 more use the /help or /about commands.",
                None,
            )
            .await?;
        }
        BotCommand::About => {
            let tg_config = tg.config.clone();

            let text = tg_config.about.clone().unwrap_or_else(|| {
                let mut paragraphs = vec![
                    r#"This bot is a part of the <a href="https://github.com/Allypost/downloader-hub/">Downloader Hub project</a>. It's a bot that helps you download your memes"#.to_string(),
                    "It is powered by Rust, yt-dlp, ffmpeg, and some external services.".to_string(),
                    "The source code is available at\nhttps://github.com/Allypost/downloader-hub/tree/main/bins/downloader-bot"
                        .to_string(),
                    "You can find out about the available extractors, downloaders and fixers by using the /list_extractors, /list_downloaders and /list_fixers commands."
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

            send_command_message(
                tg,
                msg,
                text.trim(),
                Some(LinkPreviewOptions {
                    is_disabled: true,
                    prefer_large_media: false,
                    prefer_small_media: false,
                    show_above_text: false,
                    url: None,
                }),
            )
            .await?;
        }
        BotCommand::Ping => {
            send_command_message(tg, msg, "Pong!", None).await?;
        }
        BotCommand::ListExtractors | BotCommand::ListDownloaders | BotCommand::ListFixers => {
            use crate::cmd::_common::capabilities::{CapabilityKind, fetch, render};
            let kind = match command {
                BotCommand::ListExtractors => CapabilityKind::Extractors,
                BotCommand::ListDownloaders => CapabilityKind::Downloaders,
                _ => CapabilityKind::Fixers,
            };
            let text = fetch(rpc).await.map_or_else(
                || "Failed to fetch capabilities from central.".to_string(),
                |summary| render(kind, &summary),
            );
            send_command_message(tg, msg, text, None).await?;
        }
    }

    Ok(())
}
