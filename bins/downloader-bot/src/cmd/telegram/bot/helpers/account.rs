use std::sync::Arc;

use app_database::entity::accounts::{
    AccountPlace, AccountPlaceRef, AccountUser, AccountUserRef, Platform,
};
use teloxide::{
    requests::Requester,
    types::{
        Chat, ChatFullInfo, ChatFullInfoKind, ChatFullInfoPrivate, ChatFullInfoPublicKind, ChatId,
        ChatKind, PublicChatKind, User,
    },
};

use super::super::TelegramBot;
use crate::cmd::_common::account_refresh::{PlaceFetchFut, UserFetchFut};

/// Build the end-user snapshot + ref from `msg.from`. Returns `None` for
/// channel posts and other messages with no sender (those have only a chat).
pub fn user_from_message(msg: &teloxide::types::Message) -> Option<(AccountUser, AccountUserRef)> {
    let user: &User = msg.from.as_ref()?;
    let snapshot = AccountUser {
        platform: Platform::Telegram,
        platform_id: user.id.to_string(),
        username: user.username.clone(),
        display_name: build_user_display_name(user),
        is_bot: Some(user.is_bot),
        last_seen: 0,
    };
    let r#ref = AccountUserRef {
        platform: Platform::Telegram,
        id: user.id.to_string(),
    };
    Some((snapshot, r#ref))
}

fn build_user_display_name(user: &User) -> Option<String> {
    if user.is_bot {
        return None;
    }
    let mut name = user.first_name.clone();
    if let Some(last) = &user.last_name {
        name.push(' ');
        name.push_str(last);
    }
    Some(name)
}

#[must_use]
pub fn place_from_chat(chat: &Chat) -> (AccountPlace, AccountPlaceRef) {
    let id = chat.id.to_string();
    let snapshot = AccountPlace {
        platform: Platform::Telegram,
        platform_id: id.clone(),
        kind: Some(chat_kind_str(&chat.kind).to_owned()),
        name: chat.title().map(ToString::to_string),
        username: chat.username().map(ToString::to_string),
        parent_platform_id: None,
        last_seen: 0,
    };
    let r#ref = AccountPlaceRef {
        platform: Platform::Telegram,
        id,
    };
    (snapshot, r#ref)
}

const fn chat_kind_str(kind: &ChatKind) -> &'static str {
    match kind {
        ChatKind::Private(_) => "private",
        ChatKind::Public(public) => match public.kind {
            PublicChatKind::Channel(_) => "channel",
            PublicChatKind::Group => "group",
            PublicChatKind::Supergroup(_) => "supergroup",
        },
    }
}

pub fn fetch_user_fut(bot: Arc<TelegramBot>) -> impl Fn(&str) -> UserFetchFut {
    move |platform_id| {
        let id = platform_id.to_string();
        let bot = bot.clone();
        Box::pin(async move { fetch_user_by_platform_id(&bot, &id).await })
    }
}

pub fn fetch_place_fut(bot: Arc<TelegramBot>) -> impl Fn(&str) -> PlaceFetchFut {
    move |platform_id| {
        let id = platform_id.to_string();
        let bot = bot.clone();
        Box::pin(async move { fetch_place_by_platform_id(&bot, &id).await })
    }
}

fn parse_chat_id(platform_id: &str) -> Result<ChatId, String> {
    let id: i64 = platform_id
        .parse()
        .map_err(|_| format!("invalid telegram chat id: {platform_id}"))?;
    Ok(ChatId(id))
}

pub async fn fetch_user_by_platform_id(
    bot: &TelegramBot,
    platform_id: &str,
) -> Result<AccountUser, String> {
    let chat_id = parse_chat_id(platform_id)?;
    let chat = bot
        .bot()
        .get_chat(chat_id)
        .await
        .map_err(|e| format!("get_chat failed: {e}"))?;
    user_from_chat_full(&chat)
        .ok_or_else(|| format!("chat {platform_id} is not a user private chat"))
}

pub async fn fetch_place_by_platform_id(
    bot: &TelegramBot,
    platform_id: &str,
) -> Result<AccountPlace, String> {
    let chat_id = parse_chat_id(platform_id)?;
    let chat = bot
        .bot()
        .get_chat(chat_id)
        .await
        .map_err(|e| format!("get_chat failed: {e}"))?;
    Ok(place_from_chat_full(&chat).0)
}

fn build_private_display_name(private: &ChatFullInfoPrivate) -> Option<String> {
    let first = private.first_name.as_ref()?;
    let mut name = first.clone();
    if let Some(last) = &private.last_name {
        name.push(' ');
        name.push_str(last);
    }
    Some(name)
}

fn user_from_chat_full(chat: &ChatFullInfo) -> Option<AccountUser> {
    let ChatFullInfoKind::Private(private) = &chat.kind else {
        return None;
    };
    Some(AccountUser {
        platform: Platform::Telegram,
        platform_id: chat.id.to_string(),
        username: private.username.clone(),
        display_name: build_private_display_name(private),
        is_bot: None,
        last_seen: 0,
    })
}

fn place_from_chat_full(chat: &ChatFullInfo) -> (AccountPlace, AccountPlaceRef) {
    let id = chat.id.to_string();
    let snapshot = AccountPlace {
        platform: Platform::Telegram,
        platform_id: id.clone(),
        kind: Some(chat_full_info_kind_str(&chat.kind).to_owned()),
        name: chat.title().map(ToString::to_string),
        username: chat.username().map(ToString::to_string),
        parent_platform_id: None,
        last_seen: 0,
    };
    let r#ref = AccountPlaceRef {
        platform: Platform::Telegram,
        id,
    };
    (snapshot, r#ref)
}

const fn chat_full_info_kind_str(kind: &ChatFullInfoKind) -> &'static str {
    match kind {
        ChatFullInfoKind::Private(_) => "private",
        ChatFullInfoKind::Public(public) => match public.kind {
            ChatFullInfoPublicKind::Channel(_) => "channel",
            ChatFullInfoPublicKind::Group(_) => "group",
            ChatFullInfoPublicKind::Supergroup(_) => "supergroup",
        },
    }
}
