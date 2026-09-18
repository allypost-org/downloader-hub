use std::sync::Arc;

use app_helpers::ip::url_resolves_to_valid_ip;
use app_peer_comms::message::v1::{
    central::create_result::CreateResult,
    common::{
        file::{FileReference, FileUrl},
        request_info::RequestInfo,
    },
};
use linkify::{LinkFinder, LinkKind};
use serenity::{
    all::{Message, PremiumTier},
    prelude::Context,
};
use size::Size;
use tracing::{info, warn};
use url::Url;

use crate::{
    cmd::discord::bot::{
        discord_bot::DiscordBot,
        handlers::delivery::start_request_task,
        helpers::{account, status_message::StatusMessage},
    },
    peering::rpc::RpcClient,
};

#[tracing::instrument(name = "discord-download", skip(ctx, msg, bot, rpc, urls))]
#[allow(clippy::too_many_lines)]
pub async fn handle_download_request(
    ctx: &Context,
    msg: &Message,
    bot: &Arc<DiscordBot>,
    rpc: &Arc<RpcClient>,
    mut urls: Vec<Url>,
) {
    info!(url_count = urls.len(), "Adding download request to queue");

    let max_filesize = effective_max_filesize(ctx, bot, msg).await;
    let mut status_message = StatusMessage::from_message(msg)
        .with_bot(Arc::clone(bot))
        .with_max_filesize(max_filesize);

    urls.sort();
    urls.dedup();

    if urls.is_empty() {
        status_message
            .update_message("Message doesn't contain any file or URL")
            .await;
        return;
    }

    status_message.update_message("Processing message...").await;

    let (user_snapshot, places, place_ref) = account::from_message(msg, &ctx.cache);
    {
        let mut users = Vec::new();
        if let Some((user, _)) = &user_snapshot {
            users.push(user.clone());
        }
        if let Err(e) = rpc.accounts_upsert(users, places).await {
            warn!(?e, "failed to upsert account metadata");
        }
    }
    let user_ref = user_snapshot.map(|(_, r)| r);

    let mut added_some = false;

    for (i, url) in urls.into_iter().enumerate() {
        let mut url_status_message = status_message
            .send_sub_message(&format!("Processing URL: {}", url))
            .await
            .unwrap_or_else(|| status_message.clone());

        let url_str = url.to_string();
        let validation =
            tokio::task::spawn_blocking(move || url_resolves_to_valid_ip(&url_str)).await;
        match validation {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                url_status_message
                    .update_message(&format!("Rejected URL: {}", e))
                    .await;
                continue;
            }
            Err(e) => {
                warn!(?e, ?url, "URL validation task failed");
                url_status_message
                    .update_message("Failed to validate URL")
                    .await;
                continue;
            }
        }

        let file_url: FileUrl = url.into();
        let file_url = file_url.with_max_filesize(Some(max_filesize));
        let file_ref = FileReference::url(file_url);

        let result = match rpc
            .work_request_create(
                RequestInfo::DownloadAndFix(file_ref),
                url_status_message.to_metadata(),
                Some(format!("discord-{}-{}-{}", msg.channel_id, msg.id, i)),
                user_ref.clone(),
                place_ref.clone(),
            )
            .await
        {
            Ok(CreateResult::Ok(result)) => result,
            Ok(CreateResult::Banned { reason }) => {
                url_status_message.delete_message().await;
                status_message
                    .update_message(&format!("You are banned: {reason}"))
                    .await;
                return;
            }
            Ok(res @ CreateResult::RateLimited { .. }) => {
                let msg = res
                    .rate_limit_message()
                    .unwrap_or_else(|| "Rate limit exceeded. Try again later.".to_string());
                url_status_message.delete_message().await;
                status_message.update_message(&msg).await;
                return;
            }
            Ok(_) => {
                url_status_message
                    .update_message("Failed to add URL to queue: central error")
                    .await;
                continue;
            }
            Err(e) => {
                url_status_message
                    .update_message(&format!("Failed to add URL to queue: {}", e))
                    .await;
                continue;
            }
        };

        url_status_message
            .update_message(&format!("Request added to queue with ID `{}`", result.id))
            .await;

        // Start a supervised per-request watcher for this freshly created
        // request. Not a recovery task (it was just created).
        start_request_task(
            Arc::clone(rpc),
            result.id.clone(),
            url_status_message,
            false,
        )
        .await;

        added_some = true;
    }

    if !added_some {
        status_message
            .update_message("Failed to add any requests to queue")
            .await;
        return;
    }

    status_message.delete_message().await;
}

async fn effective_max_filesize(ctx: &Context, bot: &DiscordBot, msg: &Message) -> Size {
    let premium_tier = match msg.guild_id {
        None => None,
        Some(guild_id) => {
            let cached = ctx.cache.guild(guild_id).map(|guild| guild.premium_tier);
            match cached {
                Some(tier) => Some(tier),
                None => match guild_id.to_partial_guild(&ctx.http).await {
                    Ok(guild) => Some(guild.premium_tier),
                    Err(e) => {
                        warn!(?e, ?guild_id, "failed to detect Discord guild premium tier");
                        Some(PremiumTier::Tier0)
                    }
                },
            }
        }
    };

    bot.max_payload_size()
        .min(DiscordBot::destination_max_filesize(premium_tier))
}

pub fn urls_in_message(msg: &Message) -> Vec<Url> {
    let mut urls: Vec<Url> = LinkFinder::new()
        .links(&msg.content)
        .filter(|l| matches!(l.kind(), LinkKind::Url))
        .filter_map(|l| Url::parse(l.as_str()).ok())
        .collect();

    urls.extend(
        msg.attachments
            .iter()
            .filter_map(|a| Url::parse(&a.url).ok()),
    );

    urls
}
