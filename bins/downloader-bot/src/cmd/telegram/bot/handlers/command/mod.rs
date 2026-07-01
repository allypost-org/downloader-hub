use teloxide::{
    prelude::*,
    types::{LinkPreviewOptions, Message, ReplyParameters},
    utils::command::BotCommands,
};
use tracing::{info, trace};

use crate::cmd::telegram::bot::{BotCommand, TelegramBot};

#[allow(clippy::too_many_lines)]
pub async fn handle_command(msg: &Message, command: BotCommand) -> ResponseResult<()> {
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
            let tg_config = TelegramBot::instance().config.clone();

            let text = tg_config.about.clone().unwrap_or_else(|| {
                let mut paragraphs = vec![
                    r#"This bot is a part of the <a href="https://github.com/Allypost/downloader-hub/">Downloader Hub project</a>. It's a bot that helps you download your memes"#.to_string(),
                    "It is powered by Rust, yt-dlp, ffmpeg, and some external services.".to_string(),
                    "The source code is available at\nhttps://github.com/Allypost/downloader-hub/tree/main/bins/downloader-bot"
                        .to_string(),
                    // "You can find out about the available downloaders and fixers, and what they do by using the /list_extractors, /list_downloaders and /list_fixers commands."
                    // .to_string(),
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
        BotCommand::Ping => {
            TelegramBot::instance()
                .send_message(msg.chat.id, "Pong!")
                .reply_parameters(ReplyParameters::new(msg.id).allow_sending_without_reply())
                .await?;
        }
    }

    Ok(())
}
