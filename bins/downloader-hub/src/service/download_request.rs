use std::convert::Into;

use app_entities::{
    download_request,
    entity_meta::download_request::{DownloadRequestAppMeta, DownloadRequestMeta},
    sea_orm_active_enums::ItemStatus,
};
use sea_orm::{
    AccessMode, IsolationLevel, QueryOrder, Set, TransactionError, TransactionTrait, UpdateResult,
    prelude::*, sea_query::IntoCondition,
};

use super::id::AppUidFor;
use crate::{
    queue::{TASK_QUEUE, task::Task},
    server::app_helpers::pagination::{Paginated, PaginationQuery},
};

pub struct DownloadRequestService {}
impl DownloadRequestService {
    pub async fn create_many<TDb, TValue, TPayload>(
        db: &TDb,
        payload: TPayload,
    ) -> Result<Vec<download_request::Model>, DbErr>
    where
        TDb: ConnectionTrait + TransactionTrait,
        TValue: Into<CreateDownloadRequestPayload>,
        TPayload: IntoIterator<Item = TValue> + Send + Sync,
    {
        let payloads = payload
            .into_iter()
            .map(Into::into)
            .map(|x: CreateDownloadRequestPayload| x.into_active_model())
            .collect::<Vec<_>>();

        let uids = payloads
            .iter()
            .map(|x| x.request_uid.clone().unwrap())
            .collect::<Vec<_>>();

        let requests = app_helpers::futures::retry_fn(5, || {
            let payloads = payloads.clone();
            let uids = uids.clone();

            db.transaction_with_config::<_, _, DbErr>(
                |tx| {
                    Box::pin(async move {
                        download_request::Entity::insert_many(payloads.clone())
                            .exec(tx)
                            .await?;

                        download_request::Entity::find()
                            .filter(download_request::Column::RequestUid.is_in(uids))
                            .all(tx)
                            .await
                    })
                },
                Some(IsolationLevel::Serializable),
                Some(AccessMode::ReadWrite),
            )
        })
        .await
        .map_err(|e| match e {
            TransactionError::Transaction(e) | TransactionError::Connection(e) => e,
        })?;

        for uid in uids {
            TASK_QUEUE.push(Task::download_request(uid));
        }

        Ok(requests)
    }

    pub async fn find_pending<TDb>(db: &TDb) -> Result<Vec<download_request::Model>, DbErr>
    where
        TDb: ConnectionTrait,
    {
        download_request::Entity::find()
            .filter(download_request::Column::Status.eq(ItemStatus::Pending))
            .all(db)
            .await
    }

    pub async fn update_status<TDb, TValue>(
        db: &TDb,
        uid: TValue,
        status: DownloadRequestStatus,
    ) -> Result<UpdateResult, DbErr>
    where
        TDb: ConnectionTrait,
        TValue: Into<Value> + Send + Sync,
    {
        let model = {
            let mut model = download_request::ActiveModel::new();

            if let DownloadRequestStatus::Failed(err) = &status {
                model.app_meta = Set(DownloadRequestAppMeta::Error(err.clone()).into());
            }
            model.updated_at = Set(chrono::Utc::now().into());
            model.status = Set(status.into());

            model
        };

        download_request::Entity::update_many()
            .set(model)
            .filter(download_request::Column::RequestUid.eq(uid))
            .exec(db)
            .await
    }

    pub async fn find_by_uid<TDb, TValue1>(
        db: &TDb,
        uid: TValue1,
    ) -> Result<Option<download_request::Model>, DbErr>
    where
        TDb: ConnectionTrait,
        TValue1: Into<Value> + Send + Sync,
    {
        let request = download_request::Entity::find()
            .filter(download_request::Column::RequestUid.eq(uid))
            .one(db)
            .await?;

        Ok(request)
    }

    pub async fn find_by_uid_and_client_id<TDb, TValue1, TValue2>(
        db: &TDb,
        uid: TValue1,
        client_id: TValue2,
    ) -> Result<Option<download_request::Model>, DbErr>
    where
        TDb: ConnectionTrait,
        TValue1: Into<Value> + Send + Sync,
        TValue2: Into<Value> + Send + Sync,
    {
        let request = download_request::Entity::find()
            .filter(download_request::Column::ClientId.eq(client_id))
            .filter(download_request::Column::RequestUid.eq(uid))
            .one(db)
            .await?;

        Ok(request)
    }

    pub async fn find_by_uid_with_client<TDb, TValue>(
        db: &TDb,
        uid: TValue,
    ) -> Result<Option<(download_request::Model, app_entities::client::Model)>, DbErr>
    where
        TDb: ConnectionTrait,
        TValue: Into<Value> + Send + Sync,
    {
        let res = download_request::Entity::find()
            .filter(download_request::Column::RequestUid.eq(uid))
            .find_also_related(app_entities::client::Entity)
            .one(db)
            .await?;

        let (request, client) = match res {
            Some(x) => x,
            None => return Ok(None),
        };

        let client = match client {
            Some(x) => x,
            None => return Ok(None),
        };

        Ok(Some((request, client)))
    }

    pub async fn find_all_paginated<TDb, TFilter>(
        db: &TDb,
        pagination_query: PaginationQuery,
        filter: Option<TFilter>,
    ) -> Result<Paginated<download_request::Model>, DbErr>
    where
        TDb: ConnectionTrait,
        TFilter: IntoCondition + Send + Sync,
    {
        let mut query =
            download_request::Entity::find().order_by_desc(download_request::Column::Id);

        if let Some(filter) = filter {
            query = query.filter(filter);
        }

        let paginator = query.paginate(db, pagination_query.page_size());

        Paginated::from_paginator_query(paginator, pagination_query).await
    }
}

pub enum DownloadRequestStatus {
    Failed(String),
    Pending,
    Processing,
    Success,
}
impl From<DownloadRequestStatus> for ItemStatus {
    fn from(status: DownloadRequestStatus) -> Self {
        match status {
            DownloadRequestStatus::Failed(_) => Self::Failed,
            DownloadRequestStatus::Pending => Self::Pending,
            DownloadRequestStatus::Processing => Self::Processing,
            DownloadRequestStatus::Success => Self::Success,
        }
    }
}

pub struct CreateDownloadRequestPayload {
    pub url: String,
    pub client_id: i32,
    pub meta: Option<DownloadRequestMeta>,
    pub app_meta: Option<DownloadRequestAppMeta>,
}

impl CreateDownloadRequestPayload {
    pub fn into_active_model(self) -> download_request::ActiveModel {
        let mut request = download_request::ActiveModel {
            url: Set(self.url),
            request_uid: Set(AppUidFor::download_request()),
            client_id: Set(self.client_id),
            ..Default::default()
        };
        if let Some(meta) = self.meta {
            request.meta = Set(meta.into());
        }
        if let Some(app_meta) = self.app_meta {
            request.app_meta = Set(app_meta.into());
        }

        request
    }
}
