mod handlers;

use handlers::HandlerError;
use tracing::{Instrument, Span, debug, error, field, info, info_span, warn};

use super::task::Task;
use crate::{config::Config, queue::TASK_QUEUE};

const MAX_RETRIES: u32 = 5;

pub struct TaskQueueProcessor;
impl TaskQueueProcessor {
    pub async fn run() {
        info!("Starting download request processor");
        loop {
            let task = TASK_QUEUE.pop().await;

            debug!(?task, "Got task");

            let mut status_message = task.status_message();

            let task_id = task.id().clone();

            let res = tokio::task::spawn(async move {
                handle_task(&task)
                    .instrument(info_span!(
                        "task",
                        id = ?task.id(),
                        retries = ?task.retries(),
                        chat = task.status_message().chat_id().0,
                        msg_id = task.status_message().msg_replying_to_id().0,
                        uid = field::Empty,
                        username = field::Empty,
                        name = field::Empty,
                        handler = field::Empty,
                    ))
                    .await;
            })
            .await;

            if let Err(e) = res {
                error!(?e, "Error processing task");

                let text = format!(
                    "Error processing the request!\n\nTask <code>{id}</code> \
                     failed:<pre>{err}</pre>\n\nPlease contact the <a href=\"{owner}\">bot \
                     owner</a> and forward them this message",
                    id = task_id,
                    err = e,
                    owner = Config::bot().owner_link().unwrap_or_default(),
                );
                let res = status_message.update_message(&text).await;
                if let Err(e) = res {
                    warn!(?e, "Failed to update status message");
                }
            }
        }
    }
}

async fn handle_task(task: &Task) {
    info!("Handling task");

    let handler = match handlers::HANDLERS.iter().find(|h| h.can_handle(task)) {
        Some(h) => h,
        None => {
            warn!("No handler found for task");

            let text = format!(
                "Couln't process the request, no handler found.\n\nPlease contact the <a \
                 href=\"{owner}\">bot owner</a> and forward them this message",
                owner = Config::bot().owner_link().unwrap_or_default(),
            );

            task.update_status_message(&text).await;

            return;
        }
    };

    Span::current().record("handler", field::debug(handler.name()));

    let res = handler.handle(task).await;

    let err = match res {
        Ok(returned) => {
            if let Ok(took) = task.time_since_added().to_std() {
                info!("Task completed after {:?}", took);
            }

            if returned.cleanup_status_message {
                let _ = task.status_message().delete_message().await;
            }

            return;
        }

        Err(e) => e,
    };

    if err.should_send_as_response() {
        debug!(?err, "Got error that should be sent as response");
        task.update_status_message(&err.to_string()).await;

        return;
    }

    warn!(?err, "Got error processing task");
    if let Err(e) = should_retry(task, err) {
        error!(?e, "Task will not be retried");

        let _ = task
            .status_message()
            .update_message(&format!(
                "Failed to process the request: {}\n\nPlease contact the bot owner and report \
                 this issue.",
                e
            ))
            .await;

        return;
    }

    TASK_QUEUE.push(task.retried());
}

fn should_retry(task: &Task, err: HandlerError) -> Result<(), HandlerError> {
    if task.retries() >= MAX_RETRIES {
        return Err(HandlerError::Fatal(
            "Too many retries, giving up".to_string(),
        ));
    }

    if err.is_fatal() {
        return Err(err);
    }

    Ok(())
}
