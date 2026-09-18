use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};
use size::Size;
use teloxide::{
    dispatching::dialogue::GetChatId,
    payloads::{EditMessageTextSetters, SendMessageSetters},
    requests::Requester,
    types::{ChatId, LinkPreviewOptions, Message, MessageId, ReplyParameters},
};
use tracing::{debug, trace, warn};

use super::super::TelegramBot;
use crate::cmd::telegram::bot::helpers::retried::try_send_to_retrying;

#[derive(Clone, Serialize, Deserialize)]
#[allow(clippy::struct_field_names)]
pub struct StatusMessage {
    chat_id: ChatId,
    msg_id: MessageId,
    #[serde(default)]
    reply_msg_id: Option<MessageId>,
    #[serde(default)]
    max_filesize: Option<Size>,
    #[serde(skip)]
    last_text: Option<String>,
    #[serde(skip)]
    tg: Option<Arc<TelegramBot>>,
}
impl StatusMessage {
    pub const fn new(chat_id: ChatId, msg_id: MessageId, reply_msg_id: Option<MessageId>) -> Self {
        Self {
            chat_id,
            msg_id,
            reply_msg_id,
            max_filesize: None,
            last_text: None,
            tg: None,
        }
    }

    pub fn bot(&self) -> &TelegramBot {
        self.tg
            .as_ref()
            .expect("telegram bot attached to status message")
    }

    pub fn with_bot(mut self, tg: Arc<TelegramBot>) -> Self {
        self.tg = Some(tg);
        self
    }

    pub const fn chat_id(&self) -> ChatId {
        self.chat_id
    }

    pub const fn msg_replying_to_id(&self) -> MessageId {
        self.msg_id
    }

    pub const fn status_msg_id(&self) -> Option<MessageId> {
        self.reply_msg_id
    }

    pub fn max_filesize(&self) -> Size {
        self.max_filesize
            .unwrap_or_else(|| self.bot().effective_max_filesize())
    }

    pub const fn with_max_filesize(mut self, max_filesize: Size) -> Self {
        self.max_filesize = Some(max_filesize);
        self
    }

    pub const fn from_message(msg: &Message) -> Self {
        Self::new(msg.chat.id, msg.id, None)
    }

    pub async fn send_sub_message(&self, text: &str) -> Option<Self> {
        self.try_send_sub_message(text).await.ok()
    }

    pub async fn try_send_sub_message(&self, text: &str) -> Result<Self, teloxide::RequestError> {
        let new_msg = self.try_send_additional_message(text).await?;

        let Some(chat_id) = new_msg.chat_id() else {
            debug!(chat_id = ?self.chat_id, msg_id = ?self.status_msg_id(), "Failed to send additional message: Chat not found");
            return Err(teloxide::RequestError::Api(
                teloxide::ApiError::ChatNotFound,
            ));
        };

        Ok(Self {
            chat_id,
            msg_id: self.msg_id,
            reply_msg_id: Some(new_msg.id),
            max_filesize: self.max_filesize,
            last_text: Some(text.to_string()),
            tg: self.tg.clone(),
        })
    }

    pub async fn send_additional_message(&self, text: &str) -> Option<Message> {
        self.try_send_additional_message(text).await.ok()
    }

    pub async fn try_send_additional_message(
        &self,
        text: &str,
    ) -> Result<Message, teloxide::RequestError> {
        trace!(?self.chat_id, "Sending additional message");
        let tg = self.bot_arc();
        try_send_to_retrying(
            self.chat_id,
            (text.to_string(), self.msg_id),
            Box::new(move |chat_id, (text, msg_id)| {
                let tg = Arc::clone(&tg);
                async move {
                    tg.send_message(chat_id, text)
                        .disable_notification(true)
                        .link_preview_options(LinkPreviewOptions {
                            is_disabled: true,
                            prefer_large_media: false,
                            prefer_small_media: false,
                            show_above_text: false,
                            url: None,
                        })
                        .reply_parameters(
                            ReplyParameters::new(msg_id).allow_sending_without_reply(),
                        )
                        .await
                }
            }),
        )
        .await
        .map_err(|e| {
            warn!(chat_id = ?self.chat_id, ?e, "Failed to send additional message");
            e
        })
    }

    pub async fn update_message(&mut self, text: &str) {
        if let Err(e) = self.try_update_message(text).await {
            warn!(chat_id = ?self.chat_id, msg_id = ?self.status_msg_id(), ?e, "Failed to update message");
        }
    }

    pub async fn try_update_message(&mut self, text: &str) -> Result<(), teloxide::RequestError> {
        if self.last_text.as_deref() == Some(text) {
            trace!(chat_id = ?self.chat_id, msg_id = ?self.status_msg_id(), "Skipped unchanged message");
            return Ok(());
        }

        for _ in 0..3 {
            match self.status_msg_id() {
                Some(reply_id) => {
                    let tg = self.bot_arc();
                    let res = try_send_to_retrying(
                        self.chat_id,
                        text.to_string(),
                        Box::new(move |chat_id, text| {
                            let tg = Arc::clone(&tg);
                            async move {
                                tg.edit_message_text(chat_id, reply_id, text)
                                    .link_preview_options(LinkPreviewOptions {
                                        is_disabled: true,
                                        prefer_large_media: false,
                                        prefer_small_media: false,
                                        show_above_text: false,
                                        url: None,
                                    })
                                    .await
                            }
                        }),
                    )
                    .await;

                    if matches!(
                        res,
                        Err(teloxide::RequestError::Api(
                            teloxide::ApiError::MessageToEditNotFound
                        ))
                    ) {
                        self.reply_msg_id = None;
                        continue;
                    }

                    if matches!(
                        res,
                        Err(teloxide::RequestError::Api(
                            teloxide::ApiError::MessageNotModified
                        ))
                    ) {
                        self.last_text = Some(text.to_string());
                        return Ok(());
                    }

                    if let Err(e) = res {
                        warn!(chat_id = ?self.chat_id, msg_id = ?self.status_msg_id(), ?e, "Failed to update message");
                        continue;
                    }

                    self.last_text = Some(text.to_string());
                    trace!(chat_id = ?self.chat_id, msg_id = ?self.status_msg_id(), "Updated message");

                    return Ok(());
                }
                None => {
                    let status_msg = self.try_send_additional_message(text).await?;

                    self.reply_msg_id = Some(status_msg.id);
                    self.last_text = Some(text.to_string());

                    trace!(chat_id = ?self.chat_id, msg_id = ?self.status_msg_id(), "Sent additional message");

                    return Ok(());
                }
            }
        }

        Err(teloxide::RequestError::Api(
            teloxide::ApiError::MessageNotModified,
        ))
    }

    pub async fn delete_message(&self) {
        if let Err(e) = self.try_delete_message().await {
            debug!(chat_id = ?self.chat_id, msg_id = ?self.status_msg_id(), ?e, "Failed to delete message");
        }
    }

    pub async fn try_delete_message(&self) -> Result<(), teloxide::RequestError> {
        if let Some(id) = self.status_msg_id() {
            let tg = self.bot_arc();
            try_send_to_retrying(
                self.chat_id,
                id,
                Box::new(move |chat_id, id| {
                    let tg = Arc::clone(&tg);
                    async move { tg.delete_message(chat_id, id).await }
                }),
            )
            .await?;
        }

        Ok(())
    }

    pub fn bot_arc(&self) -> Arc<TelegramBot> {
        self.tg
            .clone()
            .expect("telegram bot attached to status message")
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

impl<'a> From<&'a Message> for StatusMessage {
    fn from(msg: &'a Message) -> Self {
        Self::from_message(msg)
    }
}
