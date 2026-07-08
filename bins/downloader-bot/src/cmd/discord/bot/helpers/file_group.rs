use std::{
    future::Future,
    path::{Path, PathBuf},
};

use futures::{StreamExt, stream::FuturesUnordered};
use serenity::all::CreateAttachment;
use tokio::sync::Mutex;
use tracing::trace;

const DISCORD_MAX_ATTACHMENTS_PER_MESSAGE: usize = 10;

fn attachment_filename(path: &Path, suggested_name: Option<&Path>) -> Option<String> {
    suggested_name
        .and_then(Path::file_name)
        .or_else(|| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
}

#[tracing::instrument(skip_all)]
pub async fn send_attachment_groups<TFiles, TFile, TName, F, Fut>(
    files: TFiles,
    max_size_bytes: u64,
    mut on_group: F,
) -> Vec<(PathBuf, String)>
where
    TFiles: IntoIterator<Item = (TFile, Option<TName>)> + Send + std::fmt::Debug,
    TFile: AsRef<Path> + Send,
    TName: AsRef<Path> + Send,
    F: FnMut(Vec<CreateAttachment>) -> Fut + Send,
    Fut: Future<Output = Result<(), String>> + Send,
{
    trace!(?files, max_size_bytes, "Getting file infos");
    let (file_infos, mut failed) = infos_from_files(files).await;
    trace!(?file_infos, "Got file infos");

    let mut current_group: Vec<CreateAttachment> = Vec::new();
    let mut current_group_paths: Vec<PathBuf> = Vec::new();
    let mut current_group_size: u64 = 0;

    for info in file_infos {
        let path = info.path.clone();
        let size = info.metadata.len();

        if size > max_size_bytes {
            trace!(?path, ?size, ?max_size_bytes, "File is too large");
            failed.push((
                path,
                format!("file is too large: {} > {}", size, max_size_bytes),
            ));
            continue;
        }

        let Some(filename) = attachment_filename(&info.path, info.suggested_name.as_deref()) else {
            failed.push((info.path.clone(), "attachment has no filename".to_string()));
            continue;
        };

        let attachment = match tokio::fs::File::open(&info.path).await {
            Ok(file) => match CreateAttachment::file(&file, filename).await {
                Ok(a) => a,
                Err(e) => {
                    failed.push((info.path.clone(), format!("failed to read attachment: {e}")));
                    continue;
                }
            },
            Err(e) => {
                failed.push((info.path.clone(), format!("failed to open attachment: {e}")));
                continue;
            }
        };

        if !current_group.is_empty()
            && (current_group.len() >= DISCORD_MAX_ATTACHMENTS_PER_MESSAGE
                || current_group_size + size > max_size_bytes)
        {
            trace!(
                group_len = current_group.len(),
                current_group_size, "Flushing group"
            );
            current_group_size = 0;
            let group_paths = std::mem::take(&mut current_group_paths);
            if let Err(e) = on_group(std::mem::take(&mut current_group)).await {
                for p in group_paths {
                    failed.push((p, e.clone()));
                }
            }
        }

        current_group_paths.push(path);
        current_group.push(attachment);
        current_group_size += size;
    }

    if !current_group.is_empty() {
        trace!(group_len = current_group.len(), "Flushing final group");
        if let Err(e) = on_group(current_group).await {
            for p in current_group_paths {
                failed.push((p, e.clone()));
            }
        }
    }

    trace!(?failed, "Got final failures");

    failed
}

#[derive(Debug)]
struct FileInfo {
    path: PathBuf,
    suggested_name: Option<PathBuf>,
    metadata: std::fs::Metadata,
}

async fn infos_from_files<TFiles, TFile, TName>(
    files: TFiles,
) -> (Vec<FileInfo>, Vec<(PathBuf, String)>)
where
    TFiles: IntoIterator<Item = (TFile, Option<TName>)>,
    TFile: AsRef<Path> + Send,
    TName: AsRef<Path> + Send,
{
    let failed = std::sync::Arc::new(Mutex::new(Vec::new()));

    let infos = files
        .into_iter()
        .map(|(file, suggested_name)| {
            let file_path = file.as_ref().to_path_buf();
            let suggested_name = suggested_name.map(|name| name.as_ref().to_path_buf());
            let failed = failed.clone();

            async move {
                let metadata = match tokio::fs::metadata(&file_path).await {
                    Ok(meta) => meta,
                    Err(e) => {
                        trace!(?e, "Failed to get metadata for file");
                        failed.lock().await.push((
                            file_path.clone(),
                            "failed to get metadata for file".to_string(),
                        ));
                        return None;
                    }
                };

                Some(FileInfo {
                    path: file_path,
                    suggested_name,
                    metadata,
                })
            }
        })
        .collect::<FuturesUnordered<_>>()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    let failed = failed.lock().await.iter().cloned().collect();

    (infos, failed)
}
