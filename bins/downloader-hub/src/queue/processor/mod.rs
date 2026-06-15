use std::string::ToString;

use thiserror::Error;
use tracing::{debug, error, info, warn};

use super::task::Task;
use crate::queue::{TASK_QUEUE, task::TaskInfo};

mod download_request;
mod download_result;

const MAX_RETRIES: u32 = 5;

#[derive(Debug, Error)]
enum HandlerError {
    #[error("Join error: `{0}`")]
    JoinError(#[from] tokio::task::JoinError),
    #[error("IO error: `{0}`")]
    Io(#[from] tokio::io::Error),
    #[error("Database error: `{0}`")]
    Db(#[from] sea_orm::DbErr),
    #[error("Transaction error: `{0}`")]
    DbTransaction(#[from] sea_orm::TransactionError<sea_orm::DbErr>),
    #[error("Fatal error: `{0}`")]
    Fatal(String),
    #[error("Failed to fix: `{0}`")]
    FixFailed(#[from] app_actions::fixers::FixerError),
}
impl HandlerError {
    pub const fn is_fatal(&self) -> bool {
        matches!(self, Self::Fatal(_))
    }
}

pub struct TaskQueueProcessor;
impl TaskQueueProcessor {
    pub async fn run() {
        info!("Starting download request processor");
        loop {
            let task = TASK_QUEUE.pop().await;
            debug!(?task, "Got task");
            handle_task(&task).await;
        }
    }
}

#[tracing::instrument]
async fn handle_task(task: &Task) {
    let res = match task.info() {
        TaskInfo::DownloadRequest(uid) => download_request::handle_download_request(uid).await,
        TaskInfo::ProcessDownloadResult((request_id, path)) => {
            download_result::handle_process_result(*request_id, path.clone()).await
        }
    };

    let err = match res {
        Ok(()) => {
            if let Ok(took) = task.time_since_added().to_std() {
                info!("Task completed after {:?}", took);
            }
            return;
        }

        Err(e) => e,
    };

    warn!(?err, "Got error processing task");

    if let Err(e) = should_retry(task, err) {
        error!(?e, "Task will not be retried");
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
