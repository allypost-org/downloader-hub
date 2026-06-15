use std::path::PathBuf;

use app_entities::{client, entity_meta::common::path::AppPath};
use sea_orm::{DeleteResult, Set, prelude::*};
use serde::Deserialize;
use tracing::info;

use crate::service::id::AppUidFor;

pub struct ClientService;
impl ClientService {
    pub async fn create<TDb, TValue>(
        db: &TDb,
        payload: TValue,
    ) -> Result<client::Model, ClientCreateError>
    where
        TDb: ConnectionTrait,
        TValue: Into<ClientCreatePayload> + Send + Sync,
    {
        let payload: ClientCreatePayload = payload.into();

        let folder_path = PathBuf::from(&payload.download_folder);
        if !folder_path.exists() {
            tokio::fs::create_dir_all(&folder_path).await?;
        }
        if !folder_path.is_dir() {
            return Err(ClientCreateError::DownloadFolderNotFolder(folder_path));
        }

        info!(client = ?payload, "Adding client");
        let res = app_entities::client::ActiveModel {
            name: Set(payload.name),
            api_key: Set(AppUidFor::client()),
            download_folder: Set(AppPath::LocalAbsolute(folder_path).into()),
            ..Default::default()
        }
        .insert(db)
        .await;

        let res = match res {
            Ok(res) => res,
            Err(e) => {
                if let Some(SqlErr::UniqueConstraintViolation(_)) = e.sql_err() {
                    return Err(ClientCreateError::ClientAlreadyExists);
                }

                return Err(e.into());
            }
        };

        Ok(res)
    }

    pub async fn get_by_api_key<TDb, TValue>(
        db: &TDb,
        api_key: TValue,
    ) -> Result<Option<client::Model>, DbErr>
    where
        TDb: ConnectionTrait,
        TValue: Into<Value> + Send + Sync,
    {
        client::Entity::find()
            .filter(client::Column::ApiKey.eq(api_key))
            .one(db)
            .await
    }

    pub async fn delete_by_api_key<TDb, TValue>(
        db: &TDb,
        api_key: TValue,
    ) -> Result<DeleteResult, DbErr>
    where
        TDb: ConnectionTrait,
        TValue: Into<Value> + Send + Sync,
    {
        client::Entity::delete_many()
            .filter(client::Column::ApiKey.eq(api_key))
            .exec(db)
            .await
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCreatePayload {
    pub name: String,
    pub download_folder: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ClientCreateError {
    #[error("Client with that name already exists")]
    ClientAlreadyExists,
    #[error("Download folder {0:?} already exists and is not a directory")]
    DownloadFolderNotFolder(PathBuf),
    #[error(transparent)]
    DbErr(#[from] DbErr),
    #[error(transparent)]
    IoErr(#[from] tokio::io::Error),
}
