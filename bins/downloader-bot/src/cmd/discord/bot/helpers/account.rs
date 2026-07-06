use app_database::entity::accounts::{
    AccountPlace, AccountPlaceRef, AccountUser, AccountUserRef, Platform,
};
use serenity::{
    all::{ChannelId, GuildId, Message, User},
    cache::Cache,
};

/// Build the end-user snapshot + ref from the message author.
#[must_use]
pub fn user_from_author(author: &User) -> (AccountUser, AccountUserRef) {
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
    msg: &Message,
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
