use app_helpers::ip::url_resolves_to_valid_ip;
use app_peer_comms::{
    Message as PeerMessage,
    message::v1::{
        V1Message,
        central::{CentralMessage, create_result::CreateResult},
        common::{
            file::{FileReference, FileUrl},
            request_info::RequestInfo,
        },
    },
};
use linkify::{LinkFinder, LinkKind};
use serenity::all::Message;
use tracing::{info, trace, warn};
use url::Url;

use crate::{
    cmd::discord::bot::{discord_bot::DiscordBot, helpers::status_message::StatusMessage},
    peering::rpc::{RpcClient, RpcResponse},
};

#[tracing::instrument(name = "discord-download", skip(msg, urls))]
pub async fn handle_download_request(msg: &Message, mut urls: Vec<Url>) {
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

    let max_bytes: u64 = DiscordBot::max_payload_bytes();
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

        let resp = RpcClient::work_request_create(
            RequestInfo::DownloadAndFix(file_ref),
            url_status_message.to_metadata(),
            Some(format!("discord-{}-{}-{}", msg.channel_id, msg.id, i)),
        )
        .await;

        trace!(?resp, "Got RPC response");

        let resp = match resp {
            Ok(RpcResponse::Data(data)) => data,
            Ok(RpcResponse::Error(e)) => {
                url_status_message
                    .update_message(&format!("Failed to add URL to queue: {}", e))
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

        let Some(PeerMessage::V1(V1Message::Central(CentralMessage::WorkRequestCreateResponse(
            result,
        )))) = resp
        else {
            url_status_message
                .update_message(
                    "Failed to add request to queue: Got unknown response. Please report this to \
                     the bot developer.",
                )
                .await;
            continue;
        };

        #[allow(irrefutable_let_patterns)]
        let CreateResult::Ok(result) = result else {
            url_status_message
                .update_message("Failed to add request to queue")
                .await;
            continue;
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
