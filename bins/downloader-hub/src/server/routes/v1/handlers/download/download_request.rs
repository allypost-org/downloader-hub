use std::sync::OnceLock;

use app_entities::{
    download_request, download_result,
    entity_meta::download_request::{
        DownloadRequestAppMeta, DownloadRequestAppMetaInfo, DownloadRequestMeta,
    },
};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query},
    middleware,
    routing::get,
};
use axum_extra::extract::WithRejection;
use futures::StreamExt;
use sea_orm::prelude::*;
use serde::{Deserialize, Serialize};
use tower_http::request_id::RequestId;

use crate::{
    db::AppDb,
    server::{
        AppRouter,
        app_helpers::pagination::{Paginated, PaginationQuery},
        routes::v1::{
            middleware::auth::{CurrentUser, require_auth_not_admin},
            response::{V1Error, V1Response, V1Result},
        },
    },
    service::{
        download_request::{CreateDownloadRequestPayload, DownloadRequestService},
        signature::{Signature, WithDownloadUrl},
    },
};

pub(super) fn router() -> AppRouter {
    Router::new()
        .route("/", get(list_all).post(create_request))
        .route("/:uid", get(request_info))
        .route_layer(middleware::from_fn(require_auth_not_admin))
}

async fn list_all(
    Query(pagination_query): Query<PaginationQuery>,
    Extension(user): Extension<CurrentUser>,
) -> V1Result<Paginated<download_request::Model>> {
    let resp = DownloadRequestService::find_all_paginated(
        &AppDb::db(),
        pagination_query,
        Some(download_request::Column::ClientId.eq(user.id)),
    )
    .await?;

    Ok(V1Response::success(resp))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DownloadRequestInfoQuery {
    share_for_seconds: Option<i64>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadRequestInfoResponse {
    request: download_request::Model,
    results: Vec<WithDownloadUrl<download_result::Model>>,
}
async fn request_info(
    Extension(user): Extension<CurrentUser>,
    Path(uid): Path<String>,
    Query(query): Query<DownloadRequestInfoQuery>,
) -> V1Result<DownloadRequestInfoResponse> {
    let db = AppDb::db();

    let request = DownloadRequestService::find_by_uid_and_client_id(&db, &uid, user.id)
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
            .map(|x| {
                static MAX_DURATION_SECONDS: OnceLock<i64> = OnceLock::new();
                let max =
                    *MAX_DURATION_SECONDS.get_or_init(|| chrono::Duration::days(7).num_seconds());

                x.clamp(1, max)
            })
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
        request,
        results,
    }))
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", untagged)]
enum RequestDownloadPayload {
    Url(RequestDownloadPayloadUrl),
    Urls(Vec<RequestDownloadPayloadUrl>),
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct RequestDownloadPayloadUrl {
    url: String,
    #[serde(flatten)]
    meta: Option<DownloadRequestMeta>,
}
async fn create_request(
    Extension(user): Extension<CurrentUser>,
    Extension(request_id): Extension<RequestId>,
    WithRejection(Json(payload), _): WithRejection<Json<RequestDownloadPayload>, V1Error>,
) -> V1Result<Vec<download_request::Model>> {
    let urls = match payload {
        RequestDownloadPayload::Url(url) => vec![url],
        RequestDownloadPayload::Urls(urls) => urls,
    };

    let app_meta = Some(DownloadRequestAppMeta::Info(DownloadRequestAppMetaInfo {
        request_id: request_id
            .header_value()
            .to_str()
            .ok()
            .unwrap_or_default()
            .to_string(),
    }));

    let payloads = urls
        .into_iter()
        .map(|url| CreateDownloadRequestPayload {
            client_id: user.id,
            url: url.url,
            meta: url.meta,
            app_meta: app_meta.clone(),
        })
        .collect::<Vec<_>>();

    let requests = DownloadRequestService::create_many(&AppDb::db(), payloads).await?;

    Ok(V1Response::success(requests))
}
