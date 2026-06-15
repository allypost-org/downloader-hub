use std::path::{Path, PathBuf};

use app_actions::{download_file, fix_file};
use app_helpers::temp_dir::TempDir;
use futures::{StreamExt, stream::FuturesUnordered};
use teloxide::types::Message;
use tracing::{debug, info, trace};
use url::Url;

use super::{Handler, HandlerError, HandlerReturn};
use crate::{
    config::Config,
    queue::{
        common::{file::FileId, urls::urls_in_message},
        task::{Task, TaskInfo},
    },
};

#[derive(Debug)]
pub struct DownloadRequestHandler;
#[async_trait::async_trait]
impl Handler for DownloadRequestHandler {
    fn name(&self) -> &'static str {
        "download-request"
    }

    fn can_handle(&self, task: &Task) -> bool {
        matches!(task.info(), TaskInfo::DownloadRequest { .. })
    }

    async fn handle(&self, task: &Task) -> Result<HandlerReturn, HandlerError> {
        trace!(?task, "Handling download request");

        task.update_status_message("Processing the request...")
            .await;

        let TaskInfo::DownloadRequest { message: msg } = task.info() else {
            return Err(HandlerError::Fatal("Invalid task info".to_string()));
        };

        trace!(?msg, "Got message from task");

        task.add_span_metadata(msg);

        info!(task_id = ?task.id(), "Handling download request");

        let temp_download_dir = TempDir::in_tmp_with_prefix(format!(
            "downloader-hub.telegram-download.{}.",
            task.id()
        ))?;

        debug!("Downloading files");
        let paths_to_fix = download_files(temp_download_dir.path(), task, msg).await?;
        debug!("Downloaded files");
        trace!(?paths_to_fix, "Downloaded files");

        if paths_to_fix.is_empty() {
            task.update_status_message("No supported URL or file found in message")
                .await;

            return Ok(HandlerReturn::default().cleanup_status_message(false));
        }

        task.update_status_message("Fixing files...").await;

        trace!(?paths_to_fix, "Fixing files");
        debug!("Fixing files");
        let (fixed_file_paths, msg_to_send) = fix_files(&paths_to_fix).await?;

        if let Some(msg) = msg_to_send {
            task.send_additional_status_message(&msg).await;
        }
        debug!("Fixed files");
        trace!(?fixed_file_paths, "Fixed files");

        if let Some(owner_id) = Config::bot().owner_id
            && msg.from.as_ref().is_some_and(|user| user.id.0 == owner_id)
        {
            task.update_status_message("Copying files to download directory...")
                .await;

            debug!("Copying files to download directory");
            copy_files_to_save_dir(fixed_file_paths.clone()).await?;
            debug!("Copied files to download directory");
        }

        task.reply_with_files(fixed_file_paths)
            .await
            .map_err(HandlerError::Fatal)?;

        trace!("Deleting status message");
        let _ = task.status_message().delete_message().await;
        trace!("Status message deleted");

        Ok(HandlerReturn::default())
    }
}

#[tracing::instrument(skip_all)]
async fn copy_files_to_save_dir(fixed_file_paths: Vec<PathBuf>) -> Result<(), HandlerError> {
    let download_dir = match Config::bot().owner_download_dir.as_ref() {
        Some(x) => x,
        None => return Ok(()),
    };

    for file in fixed_file_paths {
        let Some(file_name) = file.file_name() else {
            continue;
        };
        let dest = download_dir.join(file_name);

        trace!(?file, ?dest, "Copying file to download directory");

        tokio::fs::copy(&file, &dest)
            .await
            .map_err(|e| HandlerError::Fatal(e.to_string()))?;

        trace!(?file, ?dest, "Copied file to download directory");
    }

    Ok(())
}

#[tracing::instrument(skip_all)]
async fn fix_files(
    paths_to_fix: &[PathBuf],
) -> Result<(Vec<PathBuf>, Option<String>), HandlerError> {
    let mut fixed_file_paths = vec![];
    let mut fix_errors = vec![];
    for path in paths_to_fix {
        debug!(?path, "Fixing file");

        if !path.exists() {
            return Err(HandlerError::Fatal(format!(
                "Downloaded file not found: {:?}",
                path
            )));
        }

        trace!(?path, "Fixing file");
        let res = fix_file(path).await;
        trace!(?res, "Fixed file");

        match res {
            Ok(fixed) => fixed_file_paths.push(fixed.file_path),
            Err(e) => fix_errors.push(e.to_string()),
        }
    }

    let msg_text = if fix_errors.is_empty() {
        None
    } else {
        let text = format!(
            "Failed to fix some files:\n\n{errs}",
            errs = fix_errors
                .iter()
                .map(|x| format!("- {err}", err = x))
                .collect::<Vec<_>>()
                .join("\n"),
        );

        Some(text)
    };

    return Ok((fixed_file_paths, msg_text));
}

#[tracing::instrument(skip_all)]
async fn download_files(
    download_dir: &Path,
    task: &Task,
    msg: &Message,
) -> Result<Vec<PathBuf>, HandlerError> {
    let file_id = FileId::from_message(msg);
    let file_urls = urls_in_message(msg);

    if file_id.is_none() && file_urls.is_empty() {
        return Ok(vec![]);
    }

    trace!(?file_id, ?file_urls, "Found message parts to process");

    let mut paths_to_fix = vec![];

    if let Some(file_id) = file_id {
        debug!(?file_id, "Downloading file from telegram");
        task.update_status_message("Downloading file from Telegram...")
            .await;

        trace!(?file_id, "Downloading file from telegram");
        let download_file_path = file_id
            .download(download_dir)
            .await
            .map_err(HandlerError::Fatal)?;
        trace!(?download_file_path, "Downloaded file from telegram");

        paths_to_fix.push(download_file_path);
    }

    if !file_urls.is_empty() {
        debug!(?file_urls, "Downloading files from URLs");
        task.update_status_message("Downloading files from URLs...")
            .await;

        trace!(?file_urls, "Downloading files from URLs");

        let (downloaded_file_paths, download_errors) =
            download_files_from_urls(&file_urls, download_dir).await;

        for error in download_errors {
            task.send_additional_status_message(&error).await;
        }

        trace!(?downloaded_file_paths, "Downloaded files from URLs");

        paths_to_fix.extend(downloaded_file_paths);
    }

    Ok(paths_to_fix)
}

#[tracing::instrument(skip_all, fields(download_dir))]
async fn download_files_from_urls(
    file_urls: &[Url],
    download_dir: &Path,
) -> (Vec<PathBuf>, Vec<String>) {
    let results = file_urls
        .iter()
        .map(|url| async move {
            let res = download_file(url, download_dir).await;

            (url.to_string(), res)
        })
        .collect::<FuturesUnordered<_>>()
        .collect::<Vec<_>>()
        .await;

    let mut downloaded_paths = vec![];
    let mut errors = vec![];
    for (url, url_results) in &results {
        let errs = url_results
            .iter()
            .filter_map(|x| x.as_ref().err())
            .collect::<Vec<_>>();

        if !errs.is_empty() {
            let text = format!(
                "Failed to download file from URL: {url}\n\nErrors:\n{errs}",
                url = url,
                errs = errs
                    .iter()
                    .map(|x| format!("- {err}", err = x))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );

            errors.push(text);
        }

        let paths = url_results
            .iter()
            .filter_map(|x| x.as_ref().ok())
            .map(|x| x.path.clone());

        downloaded_paths.extend(paths);
    }

    (downloaded_paths, errors)
}
