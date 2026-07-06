use app_helpers::ip::url_resolves_to_valid_ip;
use app_peer_comms::message::v1::{
    central::create_result::CreateResult,
    common::{
        file::{FileReference, FileUrl},
        request_info::RequestInfo,
    },
};
use linkify::{LinkFinder, LinkKind};
use serenity::{all::Message, prelude::Context};
use tracing::{info, warn};
use url::Url;

use crate::{
    cmd::discord::bot::{
        discord_bot::DiscordBot,
        helpers::{account, status_message::StatusMessage},
    },
    peering::rpc::RpcClient,
};

#[tracing::instrument(name = "discord-download", skip(ctx, msg, urls))]
pub async fn handle_download_request(ctx: &Context, msg: &Message, mut urls: Vec<Url>) {
    info!(url_count = urls.len(), "Adding download request to queue");

    let mut status_message = StatusMessage::from_message(msg);

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
        if let Err(e) = RpcClient::accounts_upsert(users, places).await {
            warn!(?e, "failed to upsert account metadata");
        }
    }
    let user_ref = user_snapshot.map(|(_, r)| r);

    let max_bytes = DiscordBot::max_payload_bytes();
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
        let file_url = file_url.with_max_filesize(Some(max_bytes));
        let file_ref = FileReference::url(file_url);

        let result = match RpcClient::work_request_create(
            RequestInfo::DownloadAndFix(file_ref),
            url_status_message.to_metadata(),
            Some(format!("discord-{}-{}-{}", msg.channel_id, msg.id, i)),
            user_ref.clone(),
            place_ref.clone(),
        )
        .await
        {
            Ok(CreateResult::Ok(result)) => result,
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
