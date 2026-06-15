use app_entities::{download_result, entity_meta::download_request::DownloadRequestWithHidden};
use axum::{
    Router,
    extract::{Path, Query},
    routing::get,
};
use futures::StreamExt;
use sea_orm::{Condition, ModelTrait};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    db::AppDb,
    queue::TASK_QUEUE,
    server::{
        AppRouter,
        app_helpers::pagination::{Paginated, PaginationQuery},
        routes::v1::response::{V1Response, V1Result},
    },
    service::{
        download_request::DownloadRequestService,
        signature::{Signature, WithDownloadUrl},
    },
};

pub(super) fn router() -> AppRouter {
    Router::new()
        .route("/", get(list_all))
        .route("/:uid", get(request_info))
        .route("/queue", get(queue_info))
}

async fn queue_info() -> V1Response {
    V1Response::success(json!({
        "length": TASK_QUEUE.len(),
    }))
}

async fn list_all(
    Query(pagination_query): Query<PaginationQuery>,
) -> V1Result<Paginated<DownloadRequestWithHidden>> {
    let db = AppDb::db();

    let resp =
        DownloadRequestService::find_all_paginated::<_, Condition>(&db, pagination_query, None)
            .await?;

    Ok(V1Response::success(resp.items_into()))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DownloadRequestInfoQuery {
    share_for_seconds: Option<i64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadRequestInfoResponse {
    request: DownloadRequestWithHidden,
    results: Vec<WithDownloadUrl<download_result::Model>>,
}
async fn request_info(
    Path(uid): Path<String>,
    Query(query): Query<DownloadRequestInfoQuery>,
) -> V1Result<DownloadRequestInfoResponse> {
    let db = AppDb::db();

    let request = DownloadRequestService::find_by_uid(&db, &uid)
        .await?
        .ok_or_else(V1Response::not_found)?;

    let results = {
        let stream = request
            .find_related(download_result::Entity)
            .stream(&db)
            .await;

        let stream = match stream {
            Ok(results) => results,
            Err(e) => {
                return Err(e.into());
            }
        };

        let signature_duration = query
            .share_for_seconds
            .map(|x| x.max(1))
            .map_or_else(|| chrono::Duration::hours(6), chrono::Duration::seconds);

        stream
            .filter_map(|x| async move { x.ok() })
            .map(|result| {
                let download_url = if result.status.is_success() {
                    let sig = Signature::new_expires_in(&result.result_uid, signature_duration)
                        .to_absulute_url_from_path(format!(
                            "/v1/download/results/{}/download",
                            &result.result_uid
                        ))
                        .to_string();

                    Some(sig)
                } else {
                    None
                };

                WithDownloadUrl {
                    inner: result,
                    download_url,
                }
            })
            .collect::<Vec<_>>()
            .await
    };

    Ok(V1Response::success(DownloadRequestInfoResponse {
        request: request.into(),
        results,
    }))
}
