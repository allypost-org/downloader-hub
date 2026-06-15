use std::path::PathBuf;

use app_actions::{
    actions::{ActionOptions, handlers::ActionEntry},
    fixers::handlers::FixerInstance,
};
use teloxide::{
    prelude::*,
    types::{Message, ReplyParameters},
};
use tracing::{Span, debug, field, trace, warn};

use crate::{
    bot::{TelegramBot, helpers::status_message::StatusMessage},
    queue::common::file_group::{MAX_PAYLOAD_SIZE_BYTES, files_to_input_media_groups},
};

#[derive(Clone, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum TaskInfo {
    DownloadRequest {
        message: Message,
    },
    FixRequest {
        message: Message,
        fixers: Vec<FixerInstance>,
    },
    ActionRequest {
        message: Message,
        action: ActionEntry,
        options: ActionOptions,
    },
}

#[derive(Clone, Debug)]
pub struct Task {
    id: String,
    info: TaskInfo,
    status_message: StatusMessage,
    retries: u32,
    added: chrono::DateTime<chrono::Utc>,
    last_run: Option<chrono::DateTime<chrono::Utc>>,
}
impl Task {
    pub fn download_request(message: Message, status_message: StatusMessage) -> Self {
        Self::new(TaskInfo::DownloadRequest { message }, status_message)
    }

    pub fn fix_request(
        message: Message,
        fixers: Vec<FixerInstance>,
        status_message: StatusMessage,
    ) -> Self {
        Self::new(TaskInfo::FixRequest { message, fixers }, status_message)
    }

    pub fn action_request(
        message: Message,
        action: ActionEntry,
        options: ActionOptions,
        status_message: StatusMessage,
    ) -> Self {
        Self::new(
            TaskInfo::ActionRequest {
                message,
                action,
                options,
            },
            status_message,
        )
    }
}

impl Task {
    #[tracing::instrument(skip_all)]
    pub async fn reply_with_files(&self, paths: Vec<PathBuf>) -> Result<(), String> {
        trace!("Chunking files by size");
        let (media_groups, failed_files) =
            files_to_input_media_groups(paths, MAX_PAYLOAD_SIZE_BYTES / 10 * 8).await;
        trace!(?media_groups, ?failed_files, "Chunked files by size");

        debug!("Uploading files to Telegram");
        for media_group in media_groups {
            trace!(?media_group, "Uploading media group");

            TelegramBot::instance()
                .send_media_group(self.status_message().chat_id(), media_group)
                .reply_parameters(
                    ReplyParameters::new(self.status_message().msg_replying_to_id())
                        .allow_sending_without_reply(),
                )
                .send()
                .await
                .map_err(|x| x.to_string())?;

            trace!("Uploaded media group");
        }
        debug!("Uploaded files to Telegram");

        if !failed_files.is_empty() {
            debug!(?failed_files, "Failed to chunk some files to size");
            trace!("Generating failed files message");
            let failed_files_msg = {
                let mut msg = "Failed to upload some files:\n\n".to_string();

                msg += failed_files
                    .into_iter()
                    .map(|(file, reason)| {
                        format!(
                            " - File: {}\n   Reason: {}\n",
                            file.file_name().unwrap_or_default().to_string_lossy(),
                            reason
                        )
                    })
                    .reduce(|a, b| a + "\n" + &b)
                    .unwrap_or_default()
                    .as_str();

                msg
            };
            trace!(msg = ?failed_files_msg, "Failed files message generated");

            trace!("Sending failed files message");
            self.send_additional_status_message(failed_files_msg.trim())
                .await;
            trace!("Failed files message sent");
        }

        Ok(())
    }

    #[allow(clippy::unused_self)]
    pub fn add_span_metadata(&self, msg: &Message) {
        if let Some(user) = msg.from.as_ref() {
            Span::current().record("uid", field::display(user.id.0));
            Span::current().record("name", field::debug(user.full_name()));

            if let Some(username) = user.username.as_deref() {
                Span::current().record("username", field::debug(username));
            }
        }
    }
}

impl Task {
    pub async fn update_status_message(&self, text: &str) {
        try_and_warn(
            self.status_message().update_message(text),
            "Failed to update status message",
        )
        .await;
    }

    pub async fn send_additional_status_message(&self, text: &str) {
        try_and_warn(
            self.status_message().send_additional_message(text),
            "Failed to send new status message",
        )
        .await;
    }
}

impl Task {
    pub fn new(info: TaskInfo, status_message: StatusMessage) -> Self {
        let mut id = ulid::Ulid::new().to_string();
        id.make_ascii_lowercase();

        Self {
            id,
            info,
            status_message,
            retries: 0,
            added: chrono::Utc::now(),
            last_run: None,
        }
    }

    pub const fn id(&self) -> &String {
        &self.id
    }

    pub const fn info(&self) -> &TaskInfo {
        &self.info
    }

    pub fn status_message(&self) -> StatusMessage {
        self.status_message.clone()
    }

    pub fn with_inc_retries(mut self) -> Self {
        self.retries += 1;
        self.last_run = Some(chrono::Utc::now());
        self
    }

    pub fn retried(&self) -> Self {
        self.clone().with_inc_retries()
    }

    pub const fn retries(&self) -> u32 {
        self.retries
    }

    pub fn time_since_added(&self) -> chrono::Duration {
        chrono::Utc::now().signed_duration_since(self.added)
    }
}

async fn try_and_warn<F, T, E, S>(f: F, error_msg: S) -> Option<T>
where
    F: std::future::Future<Output = Result<T, E>> + Send,
    E: std::fmt::Debug,
    S: Into<String> + Send,
{
    match f.await {
        Ok(res) => Some(res),
        Err(e) => {
            warn!(?e, "{}", error_msg.into());
            None
        }
    }
}
