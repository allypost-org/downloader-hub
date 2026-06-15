use std::{convert::Into, path::PathBuf, time::Instant};

use app_entities::{
    download_result,
    entity_meta::{
        common::path::AppPath,
        download_result::{DownloadResultMeta, DownloadResultMetaFileData, DownloadResultStatus},
    },
    sea_orm_active_enums::ItemStatusEnum,
};
use app_migration::IntoColumnRef;
use sea_orm::{InsertResult, Set, TryInsertResult, UpdateResult, prelude::*};
use tracing::{trace, warn};

use crate::service::{file::FileService, id::AppUidFor};

pub struct DownloadResultService {}
impl DownloadResultService {
    pub async fn find_by_uid<TDb, TValue>(
        db: &TDb,
        uid: TValue,
    ) -> Result<Option<download_result::Model>, DbErr>
    where
        TDb: ConnectionTrait,
        TValue: Into<String> + Send + Sync,
    {
        download_result::Entity::find()
            .filter(download_result::Column::ResultUid.eq(uid.into()))
            .one(db)
            .await
    }

    pub async fn create_many<TDb, TValue, TPayload>(
        db: &TDb,
        payload: TPayload,
    ) -> Result<InsertResult<download_result::ActiveModel>, DbErr>
    where
        TDb: ConnectionTrait,
        TValue: Into<CreateDownloadResultPayload>,
        TPayload: IntoIterator<Item = TValue> + Send + Sync,
    {
        let payload = payload
            .into_iter()
            .map(Into::into)
            .map(|x: CreateDownloadResultPayload| x.into_active_model());

        let results = download_result::Entity::insert_many(payload)
            .on_empty_do_nothing()
            .exec(db)
            .await?;

        let results = match results {
            TryInsertResult::Inserted(r) => r,
            TryInsertResult::Empty => return Ok(InsertResult { last_insert_id: 0 }),
            TryInsertResult::Conflicted => return Err(DbErr::RecordNotInserted),
        };

        Ok(results)
    }

    pub async fn add_app_meta<TDb, TPath>(
        db: &TDb,
        request_id: i32,
        file_path: TPath,
    ) -> Result<UpdateResult, anyhow::Error>
    where
        TDb: ConnectionTrait,
        TPath: Into<AppPath> + Send + Sync + std::fmt::Debug,
    {
        #[allow(clippy::match_wildcard_for_single_variants)]
        let file_path = match file_path.into() {
            AppPath::LocalAbsolute(x) => x,
            x => return Err(anyhow::anyhow!("Cannot add app meta for {:?}", x)),
        };
        let meta = tokio::fs::metadata(&file_path).await?;

        if !meta.is_file() {
            anyhow::bail!("Path is not a file: {:?}", &file_path);
        }

        let hash = {
            let now = Instant::now();
            let hash = FileService::file_hash(&file_path).await;
            trace!(?hash, took = ?now.elapsed(), "Calculated file hash");
            hash?
        };

        let file_type = match FileService::infer_file_type(&file_path).await {
            Ok(x) => Some(x),
            Err(e) => {
                warn!(err = ?e, "Failed to infer file type");
                None
            }
        };

        let size: Option<i64> = meta.len().try_into().ok();

        let app_meta = DownloadResultMeta::FileData(DownloadResultMetaFileData {
            hash,
            size,
            file_type,
        });

        Self::update_app_meta(db, request_id, AppPath::LocalAbsolute(file_path), app_meta)
            .await
            .map_err(Into::into)
    }

    pub async fn update_app_meta<TDb, TPath, TValue>(
        db: &TDb,
        request_id: i32,
        file_path: TPath,
        new_meta: TValue,
    ) -> Result<UpdateResult, DbErr>
    where
        TDb: ConnectionTrait,
        TPath: Into<AppPath> + Send + Sync + std::fmt::Debug,
        TValue: Into<DownloadResultMeta> + Send + Sync + std::fmt::Debug,
    {
        let new_meta: DownloadResultMeta = new_meta.into();

        download_result::Entity::update_many()
            .col_expr(
                download_result::Column::Meta,
                Expr::cust_with_exprs(
                    "coalesce($1, '{}') || $2",
                    [
                        download_result::Column::Meta.into_column_ref().into(),
                        Expr::value(serde_json::to_value(new_meta).expect("Invalid meta value")),
                    ],
                ),
            )
            .col_expr(
                download_result::Column::UpdatedAt,
                Expr::value(Expr::current_timestamp()),
            )
            .filter(download_result::Column::DownloadRequestId.eq(request_id))
            .filter(
                download_result::Column::Path
                    .eq(serde_json::to_value(file_path.into()).expect("Invalid path value")),
            )
            .exec(db)
            .await
    }

    pub async fn update_path<TDb, TPath>(
        db: &TDb,
        request_id: i32,
        old_path: TPath,
        new_path: TPath,
    ) -> Result<UpdateResult, DbErr>
    where
        TDb: ConnectionTrait,
        TPath: Into<AppPath> + Send + Sync,
    {
        download_result::Entity::update_many()
            .col_expr(
                download_result::Column::Path,
                Expr::value(serde_json::to_value(new_path.into()).expect("Invalid path value")),
            )
            .col_expr(
                download_result::Column::UpdatedAt,
                Expr::value(Expr::current_timestamp()),
            )
            .filter(download_result::Column::DownloadRequestId.eq(request_id))
            .filter(
                download_result::Column::Path
                    .eq(serde_json::to_value(old_path.into()).expect("Invalid path value")),
            )
            .exec(db)
            .await
    }

    pub async fn update_status<TDb, TValue, TPath>(
        db: &TDb,
        request_id: TValue,
        path: TPath,
        status: DownloadResultStatus,
    ) -> Result<UpdateResult, DbErr>
    where
        TDb: ConnectionTrait,
        TValue: Into<i32> + Send + Sync,
        TPath: Into<AppPath> + Send + Sync,
    {
        download_result::Entity::update_many()
            .col_expr(
                download_result::Column::Status,
                Expr::value(status.as_item_status()).cast_as(ItemStatusEnum),
            )
            .col_expr(
                download_result::Column::UpdatedAt,
                Expr::value(Expr::current_timestamp()),
            )
            .filter(download_result::Column::DownloadRequestId.eq(request_id.into()))
            .filter(
                download_result::Column::Path
                    .eq(serde_json::to_value(path.into()).expect("Invalid path value")),
            )
            .exec(db)
            .await
    }

    pub async fn find_pending_results<TDb>(db: &TDb) -> Result<Vec<download_result::Model>, DbErr>
    where
        TDb: ConnectionTrait,
    {
        download_result::Entity::find()
            .filter(
                download_result::Column::Status.eq(DownloadResultStatus::Pending.as_item_status()),
            )
            .all(db)
            .await
    }
}

pub struct CreateDownloadResultPayload {
    pub request_id: i32,
    pub status: DownloadResultStatus,
    pub path: Option<PathBuf>,
    pub meta: Option<DownloadResultMeta>,
}
impl CreateDownloadResultPayload {
    pub fn into_active_model(self) -> download_result::ActiveModel {
        let mut model = download_result::ActiveModel {
            download_request_id: Set(self.request_id),
            result_uid: Set(AppUidFor::download_result()),
            status: Set(self.status.as_item_status()),
            path: Set(self.path.map(AppPath::LocalAbsolute).map(Into::into)),
            ..Default::default()
        };

        if let Some(meta) = self.meta {
            model.meta = Set(meta.into());
        }

        model
    }
}
