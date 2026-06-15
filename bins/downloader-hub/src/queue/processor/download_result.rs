use app_actions::fix_file;
use app_entities::entity_meta::{
    common::path::AppPath,
    download_result::{DownloadResultMeta, DownloadResultStatus},
};
use sea_orm::{DbErr, TransactionTrait};
use tracing::{debug, error};

use super::HandlerError;
use crate::{db::AppDb, service::download_result::DownloadResultService};

pub async fn handle_process_result(request_id: i32, path: AppPath) -> Result<(), HandlerError> {
    match fix(request_id, path.clone()).await {
        Ok(()) => Ok(()),
        Err(e) if e.is_fatal() => {
            let err = DownloadResultService::update_status(
                &AppDb::db(),
                request_id,
                path,
                DownloadResultStatus::Failed(e.to_string()),
            )
            .await;

            if let Err(e) = err {
                error!(?e, "Failed to update download result");
            }

            Err(e)
        }
        Err(e) => {
            let err = DownloadResultService::update_status(
                &AppDb::db(),
                request_id,
                path,
                DownloadResultStatus::Pending,
            )
            .await;

            if let Err(e) = err {
                error!(?e, "Failed to update download result");
            }

            Err(e)
        }
    }
}

async fn fix(request_id: i32, app_path: AppPath) -> Result<(), HandlerError> {
    debug!(?app_path, "Fixing file");

    #[allow(clippy::match_wildcard_for_single_variants)]
    let path = match app_path.clone() {
        AppPath::LocalAbsolute(path) => path,
        _ => {
            return Err(HandlerError::Fatal(format!(
                "Cannot process path: {:?}",
                app_path
            )));
        }
    };

    let db = AppDb::db();

    DownloadResultService::update_status(
        &db,
        request_id,
        app_path.clone(),
        DownloadResultStatus::Processing,
    )
    .await?;

    let new_path = fix_file(&path).await;

    match new_path {
        Err(e) => {
            DownloadResultService::update_app_meta(
                &db,
                request_id,
                AppPath::LocalAbsolute(path.clone()),
                DownloadResultMeta::Error(e.to_string()),
            )
            .await?;
        }
        Ok(new_path) => {
            let new_path = &new_path.file_path;
            app_helpers::futures::retry_fn(5, || async {
                let path = path.clone();
                let new_path = new_path.clone();

                db.transaction_with_config::<_, _, DbErr>(
                    |tx| {
                        Box::pin(async move {
                            DownloadResultService::update_path(
                                tx,
                                request_id,
                                AppPath::LocalAbsolute(path.clone()),
                                AppPath::LocalAbsolute(new_path.clone()),
                            )
                            .await?;

                            DownloadResultService::update_status(
                                tx,
                                request_id,
                                AppPath::LocalAbsolute(new_path.clone()),
                                DownloadResultStatus::Success,
                            )
                            .await?;

                            Ok(())
                        })
                    },
                    Some(sea_orm::IsolationLevel::Serializable),
                    Some(sea_orm::AccessMode::ReadWrite),
                )
                .await
            })
            .await?;
        }
    }

    Ok(())
}
