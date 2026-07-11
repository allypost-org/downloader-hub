use app_peer_comms::message::v1::{
    central::create_result::CreateResult,
    common::{
        file::{FileReference, FileUrl},
        request_info::RequestInfo,
    },
};
use teloxide::{
    prelude::*,
    types::{Message as TelegramMessage, MessageEntityKind},
};
use tracing::{info, warn};
use url::Url;

use crate::{
    cmd::telegram::bot::{
        TelegramBot,
        handlers::delivery::start_request_task,
        helpers,
        helpers::{file_id::FileId, status_message::StatusMessage},
    },
    peering::rpc::RpcClient,
};

pub async fn handle_message(msg: &TelegramMessage) -> ResponseResult<()> {
    info!("Adding download request to queue");

    let mut status_message = StatusMessage::from_message(msg);

    let file_id = FileId::from_message(msg);
    let file_urls = {
        let mut urls = urls_in_message(msg);
        urls.sort();
        urls
    };

    if file_id.is_none() && file_urls.is_empty() {
        status_message
            .update_message("Message doesn't contain any file or URL")
            .await;

        return Ok(());
    }

    status_message.update_message("Processing message...").await;

    let (place_snapshot, place_ref) = helpers::account::place_from_chat(&msg.chat);
    let user_snapshot = helpers::account::user_from_message(msg);
    {
        let mut users = Vec::new();
        let places = vec![place_snapshot.clone()];
        if let Some((user, _)) = &user_snapshot {
            users.push(user.clone());
        }
        if let Err(e) = RpcClient::accounts_upsert(users, places).await {
            warn!(?e, "failed to upsert account metadata");
        }
    }
    let user_ref = user_snapshot.map(|(_, r)| r);

    let mut added_some = false;
    for (i, file_url) in file_urls.into_iter().enumerate() {
        let mut url_status_message = status_message
            .send_sub_message(&format!("Processing URL: {}", file_url))
            .await
            .unwrap_or_else(|| status_message.clone());

        let result = match RpcClient::work_request_create(
            RequestInfo::DownloadAndFix({
                let file_url: FileUrl = file_url.into();

                FileReference::url(
                    file_url.with_max_filesize(Some(TelegramBot::max_payload_size())),
                )
            }),
            url_status_message.to_metadata(),
            Some(format!("tg-{}-{}-{}", msg.chat.id, msg.id, i)),
            user_ref.clone(),
            Some(place_ref.clone()),
        )
        .await
        {
            Ok(CreateResult::Ok(result)) => result,
            Ok(CreateResult::Banned { reason }) => {
                url_status_message.delete_message().await;
                status_message
                    .update_message(&format!("You are banned: {reason}"))
                    .await;
                return Ok(());
            }
            Ok(res @ CreateResult::RateLimited { .. }) => {
                let msg = res
                    .rate_limit_message()
                    .unwrap_or_else(|| "Rate limit exceeded. Try again later.".to_string());
                url_status_message.delete_message().await;
                status_message.update_message(&msg).await;
                return Ok(());
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
            .update_message(&format!(
                "Request added to queue with ID <code>{}</code>",
                result.id
            ))
            .await;

        // Start a supervised per-request watcher for this freshly created
        // request. Not a recovery task (it was just created).
        start_request_task(result.id.clone(), url_status_message, false).await;

        added_some = true;
    }

    if !added_some {
        status_message
            .update_message("Failed to add any requests to queue")
            .await;

        return Ok(());
    }

    status_message.delete_message().await;

    Ok(())
}

pub fn urls_in_message(msg: &TelegramMessage) -> Vec<Url> {
    let entities = msg
        .parse_entities()
        .or_else(|| msg.parse_caption_entities())
        .unwrap_or_default();

    entities
        .iter()
        .filter_map(|x| match x.kind() {
            MessageEntityKind::Url => Url::parse(x.text()).ok(),
            MessageEntityKind::TextLink { url } => Some(url.clone()),
            _ => None,
        })
        .collect()
}
