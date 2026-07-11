use std::future::Future;
use std::pin::Pin;

use app_database::entity::accounts::{
    AccountPlace, AccountPlaceRef, AccountUser, AccountUserRef, Platform,
};
use serenity::all::{Channel, ChannelId, GuildId, UserId};
use serenity::cache::Cache;

use super::super::discord_bot::DiscordBot;

/// Build the end-user snapshot + ref from the message author.
#[must_use]
pub fn user_from_author(author: &serenity::all::User) -> (AccountUser, AccountUserRef) {
    let id = author.id.to_string();
    let snapshot = AccountUser {
        platform: Platform::Discord,
        platform_id: id.clone(),
        username: Some(author.name.clone()),
        display_name: author.global_name.clone(),
        is_bot: Some(author.bot),
        last_seen: 0,
    };
    let r#ref = AccountUserRef {
        platform: Platform::Discord,
        id,
    };
    (snapshot, r#ref)
}

/// Resolve the channel + (when present) the guild from the cache, returning
/// snapshot rows ready to upsert. The channel's `parent_platform_id` is set
/// to the guild id when the message came from a guild channel.
///
/// Cache misses return only what's available from the message itself
/// (channel id + guild id) so the request never fails on metadata lookup.
pub fn places_from_cache(
    cache: &Cache,
    channel_id: ChannelId,
    guild_id: Option<GuildId>,
) -> (Vec<AccountPlace>, AccountPlaceRef) {
    let mut places = Vec::new();

    let channel_place = build_channel_place(cache, channel_id, guild_id);
    let place_ref = AccountPlaceRef {
        platform: Platform::Discord,
        id: channel_place.platform_id.clone(),
    };

    places.push(channel_place);

    if let Some(guild_id) = guild_id
        && let Some(guild) = cache.guild(guild_id)
    {
        places.push(AccountPlace {
            platform: Platform::Discord,
            platform_id: guild_id.to_string(),
            kind: Some("server".to_owned()),
            name: Some(guild.name.clone()),
            username: None,
            parent_platform_id: None,
            last_seen: 0,
        });
    }

    (places, place_ref)
}

fn build_channel_place(
    cache: &Cache,
    channel_id: ChannelId,
    guild_id: Option<GuildId>,
) -> AccountPlace {
    if let Some(guild_id) = guild_id
        && let Some(guild) = cache.guild(guild_id)
        && let Some(channel) = guild.channels.get(&channel_id)
    {
        return AccountPlace {
            platform: Platform::Discord,
            platform_id: channel_id.to_string(),
            kind: Some(channel_kind_str(channel.kind).to_owned()),
            name: Some(channel.name.clone()),
            username: None,
            parent_platform_id: Some(guild_id.to_string()),
            last_seen: 0,
        };
    }
    AccountPlace {
        platform: Platform::Discord,
        platform_id: channel_id.to_string(),
        kind: guild_id
            .map(|_| "text".to_owned())
            .or_else(|| Some("dm".to_owned())),
        name: None,
        username: None,
        parent_platform_id: guild_id.map(|g| g.to_string()),
        last_seen: 0,
    }
}

const fn channel_kind_str(kind: serenity::all::ChannelType) -> &'static str {
    use serenity::all::ChannelType as C;
    match kind {
        C::Text => "text",
        C::Private => "dm",
        C::Voice => "voice",
        C::GroupDm => "group",
        C::Category => "category",
        C::News => "news",
        C::NewsThread | C::PublicThread | C::PrivateThread => "thread",
        C::Stage => "stage",
        C::Forum => "forum",
        C::Directory => "directory",
        _ => "other",
    }
}

/// Convenience wrapper used by the message handler.
pub fn from_message(
    msg: &serenity::all::Message,
    cache: &Cache,
) -> (
    Option<(AccountUser, AccountUserRef)>,
    Vec<AccountPlace>,
    Option<AccountPlaceRef>,
) {
    let user = if msg.author.bot {
        None
    } else {
        Some(user_from_author(&msg.author))
    };
    let (places, place_ref) = places_from_cache(cache, msg.channel_id, msg.guild_id);
    (user, places, Some(place_ref))
}

pub fn fetch_user_fut(platform_id: &str) -> Pin<Box<dyn Future<Output = Result<AccountUser, String>> + Send>> {
    let id = platform_id.to_string();
    Box::pin(async move { fetch_user_by_platform_id(&id).await })
}

pub fn fetch_place_fut(platform_id: &str) -> Pin<Box<dyn Future<Output = Result<AccountPlace, String>> + Send>> {
    let id = platform_id.to_string();
    Box::pin(async move { fetch_place_by_platform_id(&id).await })
}

pub async fn fetch_user_by_platform_id(platform_id: &str) -> Result<AccountUser, String> {
    let id = platform_id
        .parse::<u64>()
        .map_err(|_| format!("invalid discord user id: {platform_id}"))?;
    let user = DiscordBot::bot()
        .get_user(UserId::new(id))
        .await
        .map_err(|e| format!("get_user failed: {e}"))?;
    Ok(user_from_author(&user).0)
}

pub async fn fetch_place_by_platform_id(platform_id: &str) -> Result<AccountPlace, String> {
    let id = platform_id
        .parse::<u64>()
        .map_err(|_| format!("invalid discord place id: {platform_id}"))?;
    let http = DiscordBot::bot();
    let channel_id = ChannelId::new(id);
    if let Ok(channel) = http.get_channel(channel_id).await {
        return Ok(place_from_channel(&channel));
    }
    let guild_id = GuildId::new(id);
    let guild = http
        .get_guild(guild_id)
        .await
        .map_err(|e| format!("get_channel/get_guild failed: {e}"))?;
    Ok(AccountPlace {
        platform: Platform::Discord,
        platform_id: guild_id.to_string(),
        kind: Some("server".to_owned()),
        name: Some(guild.name.clone()),
        username: None,
        parent_platform_id: None,
        last_seen: 0,
    })
}

fn place_from_channel(channel: &Channel) -> AccountPlace {
    match channel {
        Channel::Guild(guild_channel) => AccountPlace {
            platform: Platform::Discord,
            platform_id: guild_channel.id.to_string(),
            kind: Some(channel_kind_str(guild_channel.kind).to_owned()),
            name: Some(guild_channel.name.clone()),
            username: None,
            parent_platform_id: Some(guild_channel.guild_id.to_string()),
            last_seen: 0,
        },
        Channel::Private(dm) => AccountPlace {
            platform: Platform::Discord,
            platform_id: dm.id.to_string(),
            kind: Some("dm".to_owned()),
            name: Some(dm.name()),
            username: None,
            parent_platform_id: None,
            last_seen: 0,
        },
        _ => AccountPlace {
            platform: Platform::Discord,
            platform_id: channel.id().to_string(),
            kind: Some("other".to_owned()),
            name: None,
            username: None,
            parent_platform_id: None,
            last_seen: 0,
        },
    }
}
