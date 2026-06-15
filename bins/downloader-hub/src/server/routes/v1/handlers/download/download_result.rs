use app_entities::{
    download_result,
    entity_meta::{common::path::AppPath, download_result::DownloadResultMeta},
    sea_orm_active_enums::ItemStatus,
};
use axum::{
    Router,
    extract::{OriginalUri, Path, Query},
    http::{HeaderMap, StatusCode, header},
    middleware,
    response::IntoResponse,
    routing::get,
};
use serde::Deserialize;
use tracing::{error, trace};

use crate::{
    db::AppDb,
    server::{
        AppRouter,
        app_response::range_responder::RangeResponder,
        routes::v1::{
            middleware::auth::require_auth,
            response::{V1Response, V1Result},
        },
    },
    service::{
        download_result::DownloadResultService,
        signature::{Signature, WithDownloadUrl},
    },
};

pub(super) fn router() -> AppRouter {
    Router::new()
        .route("/:result_uid", get(get_result_info))
        .route_layer(middleware::from_fn(require_auth))
        .route("/:result_uid/download", get(download_result))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetResultInfoQuery {
    share_for_seconds: Option<i64>,
}
async fn get_result_info(
    Path(result_uid): Path<String>,
    OriginalUri(uri): OriginalUri,
    Query(query): Query<GetResultInfoQuery>,
) -> V1Result<WithDownloadUrl<download_result::Model>> {
    let result = DownloadResultService::find_by_uid(&AppDb::db(), &result_uid)
        .await?
        .ok_or_else(V1Response::not_found)?;

    let sig = Signature::new_expires_in(
        &result_uid,
        query
            .share_for_seconds
            .map_or_else(|| chrono::Duration::hours(6), chrono::Duration::seconds),
    );

    Ok(V1Response::Success(WithDownloadUrl {
        inner: result,
        download_url: Some(
            sig.to_absulute_url_from_path(format!("{}/download", uri.path()))
                .to_string(),
        ),
    }))
}

async fn download_result(
    Path(result_uid): Path<String>,
    headers: HeaderMap,
    signature: Query<Signature>,
) -> impl IntoResponse {
    trace!(?signature, "Got signature");

    if let Err(e) = signature.validate(&result_uid) {
        return Err((StatusCode::BAD_REQUEST, format!("Invalid signature: {}", e)).into_response());
    }

    let db = AppDb::db();
    let result = DownloadResultService::find_by_uid(&db, result_uid)
        .await
        .map_err(|e| {
            error!(?e, "Failed to find download result");

            (StatusCode::INTERNAL_SERVER_ERROR).into_response()
        })?
        .ok_or_else(|| (StatusCode::NOT_FOUND).into_response())?;

    if result.status != ItemStatus::Success {
        return Err((
            StatusCode::CONFLICT,
            [(
                "X-Item-Status",
                serde_json::to_string(&result.status).unwrap_or_default(),
            )],
        )
            .into_response());
    }

    let result_meta = result.meta().map(|x| match x {
        DownloadResultMeta::FileData(_) => Ok(x),
        DownloadResultMeta::Error(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e).into_response()),
    });

    if let Some(Err(e)) = result_meta {
        return Err(e);
    }

    let file_path = {
        let path = match result.path() {
            Some(path) => path,
            None => return Err((StatusCode::NOT_FOUND).into_response()),
        };

        #[allow(clippy::match_wildcard_for_single_variants)]
        let path = match path {
            AppPath::LocalAbsolute(path) => path,
            _ => return Err((StatusCode::INTERNAL_SERVER_ERROR).into_response()),
        };

        if !path.exists() {
            return Err((StatusCode::NOT_FOUND).into_response());
        }

        path
    };

    let mut responder = RangeResponder::from_path(&file_path);

    responder
        .add_header(
            header::CACHE_CONTROL,
            "public, max-age=31536000, immutable"
                .parse()
                .expect("Invalid cache control header value"),
        )
        .add_header(
            header::CONTENT_DISPOSITION,
            format!(
                "inline; filename={:?}",
                file_path.file_name().unwrap_or_default()
            )
            .parse()
            .expect("Invalid content disposition header value"),
        )
        .add_header(
            header::PRAGMA,
            "public".parse().expect("Invalid pragma header value"),
        );

    if let Some(Ok(result_meta)) = result_meta {
        match result_meta {
            DownloadResultMeta::FileData(x) => {
                if let Ok(etag) = format!("{:?}", x.hash).parse() {
                    responder.add_header(header::ETAG, etag);
                }
            }
            DownloadResultMeta::Error(_) => {}
        }
    }

    let resp = responder.into_response(headers).await;

    Ok(resp)
}
