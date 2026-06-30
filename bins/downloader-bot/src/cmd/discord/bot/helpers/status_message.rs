use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serenity::{
    all::{
        ChannelId, CreateMessage, EditMessage, Message, MessageId, MessageReference,
        MessageReferenceKind, UserId,
    },
    http::{Http, HttpError},
};
use tracing::{debug, trace, warn};

use super::super::discord_bot::DiscordBot;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_field_names)]
pub struct StatusMessage {
    channel_id: ChannelId,
    msg_id: MessageId,
    author_id: UserId,
    #[serde(default)]
    status_msg_id: Option<MessageId>,
}

impl StatusMessage {
    pub const fn new(
        channel_id: ChannelId,
        msg_id: MessageId,
        author_id: UserId,
        status_msg_id: Option<MessageId>,
    ) -> Self {
        Self {
            channel_id,
            msg_id,
            author_id,
            status_msg_id,
        }
    }

    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    pub fn reply_target(&self) -> MessageId {
        self.status_msg_id.unwrap_or(self.msg_id)
    }

    pub fn reply_reference(&self) -> MessageReference {
        MessageReference::new(MessageReferenceKind::default(), self.channel_id)
            .message_id(self.reply_target())
            .fail_if_not_exists(false)
    }

    pub const fn author_id(&self) -> UserId {
        self.author_id
    }

    pub const fn status_msg_id(&self) -> Option<MessageId> {
        self.status_msg_id
    }

    pub const fn from_message(msg: &Message) -> Self {
        Self::new(msg.channel_id, msg.id, msg.author.id, None)
    }

    fn http() -> &'static Http {
        DiscordBot::bot()
    }

    pub async fn send_sub_message(&self, text: &str) -> Option<Self> {
        self.try_send_sub_message(text).await.ok()
    }

    async fn try_send_sub_message(&self, text: &str) -> Result<Self, serenity::Error> {
        let new_msg = self.try_send_additional_message(text).await?;

        Ok(Self {
            channel_id: new_msg.channel_id,
            msg_id: self.msg_id,
            author_id: self.author_id,
            status_msg_id: Some(new_msg.id),
        })
    }

    pub async fn send_additional_message(&self, text: &str) -> Option<Message> {
        self.try_send_additional_message(text).await.ok()
    }

    async fn try_send_additional_message(&self, text: &str) -> Result<Message, serenity::Error> {
        trace!(channel_id = ?self.channel_id, "Sending additional message");
        let builder = CreateMessage::new()
            .content(text)
            .reference_message(self.reply_reference());
        self.channel_id
            .send_message(Self::http(), builder)
            .await
            .map_err(|e| {
                warn!(channel_id = ?self.channel_id, ?e, "Failed to send additional message");
                e
            })
    }

    pub async fn update_message(&mut self, text: &str) {
        if let Err(e) = self.try_update_message(text).await {
            warn!(
                channel_id = ?self.channel_id,
                msg_id = ?self.status_msg_id(),
                ?e,
                "Failed to update message"
            );
        }
    }

    async fn try_update_message(&mut self, text: &str) -> Result<(), serenity::Error> {
        for _ in 0..3 {
            match self.status_msg_id {
                Some(msg_id) => {
                    let builder = EditMessage::new().content(text);
                    let res = self
                        .channel_id
                        .edit_message(Self::http(), msg_id, builder)
                        .await;

                    if Self::is_unknown_message_err(&res) {
                        debug!(
                            channel_id = ?self.channel_id,
                            msg_id = ?msg_id,
                            "Status message disappeared, will resend"
                        );
                        self.status_msg_id = None;
                        continue;
                    }

                    if let Err(e) = res {
                        warn!(
                            channel_id = ?self.channel_id,
                            msg_id = ?msg_id,
                            ?e,
                            "Failed to update message"
                        );
                        continue;
                    }

                    trace!(
                        channel_id = ?self.channel_id,
                        msg_id = ?msg_id,
                        "Updated message"
                    );
                    return Ok(());
                }
                None => {
                    let status_msg = self.try_send_additional_message(text).await?;
                    self.status_msg_id = Some(status_msg.id);

                    trace!(
                        channel_id = ?self.channel_id,
                        msg_id = ?self.status_msg_id(),
                        "Sent additional message"
                    );
                    return Ok(());
                }
            }
        }

        Err(Self::unknown_message_err())
    }

    pub async fn delete_message(&self) {
        if let Err(e) = self.try_delete_message().await {
            debug!(
                channel_id = ?self.channel_id,
                msg_id = ?self.status_msg_id(),
                ?e,
                "Failed to delete message"
            );
        }
    }

    async fn try_delete_message(&self) -> Result<(), serenity::Error> {
        if let Some(id) = self.status_msg_id {
            self.channel_id.delete_message(Self::http(), id).await?;
        }
        Ok(())
    }

    const fn is_unknown_message_err(res: &Result<Message, serenity::Error>) -> bool {
        match res {
            Err(serenity::Error::Http(HttpError::UnsuccessfulRequest(err))) => {
                err.status_code.as_u16() == 404 || err.error.code == 10008
            }
            _ => false,
        }
    }

    const fn unknown_message_err() -> serenity::Error {
        serenity::Error::Other("Status message not found")
    }
}

impl StatusMessage {
    pub fn to_metadata(&self) -> HashMap<String, String> {
        let s = serde_json::to_string(self).expect("Failed to serialize status message");
        HashMap::from_iter([("status_message".to_string(), s)])
    }

    pub fn from_metadata(metadata: &HashMap<String, String>) -> Result<Self, serde_json::Error> {
        let s = metadata.get("status_message").ok_or_else(|| {
            <serde_json::Error as serde::de::Error>::custom("Missing status message")
        })?;

        serde_json::from_str(s)
    }
}

impl From<Message> for StatusMessage {
    fn from(msg: Message) -> Self {
        Self::from_message(&msg)
    }
}

impl From<&Message> for StatusMessage {
    fn from(msg: &Message) -> Self {
        Self::from_message(msg)
    }
}
