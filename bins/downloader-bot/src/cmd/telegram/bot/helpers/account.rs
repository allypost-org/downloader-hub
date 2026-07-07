use app_database::entity::accounts::{
    AccountPlace, AccountPlaceRef, AccountUser, AccountUserRef, Platform,
};
use teloxide::types::{Chat, ChatKind, Message, PublicChatKind, User};

/// Build the end-user snapshot + ref from `msg.from`. Returns `None` for
/// channel posts and other messages with no sender (those have only a chat).
pub fn user_from_message(msg: &Message) -> Option<(AccountUser, AccountUserRef)> {
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

/// Build the place (chat) snapshot + ref.
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
