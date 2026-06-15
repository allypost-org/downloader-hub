use app_entities::entity_meta::client::ClientWithHidden;
use axum::{
    Json, Router,
    extract::{Path, Query},
    http::StatusCode,
    routing::get,
};
use axum_extra::extract::WithRejection;
use sea_orm::{QueryOrder, prelude::*};

use crate::{
    db::AppDb,
    server::{
        AppRouter,
        app_helpers::pagination::{Paginated, PaginationQuery},
        routes::v1::response::{V1Error, V1Response, V1Result},
    },
    service::client::{ClientCreateError, ClientCreatePayload, ClientService},
};

pub(super) fn router() -> AppRouter {
    Router::new()
        .route("/", get(list_clients).put(add_client))
        .route("/:api_key", get(get_client).delete(remove_client))
}

async fn list_clients(
    Query(pagination_query): Query<PaginationQuery>,
) -> V1Result<Paginated<ClientWithHidden>> {
    let db = AppDb::db();
    let paginator = app_entities::client::Entity::find()
        .order_by_desc(app_entities::client::Column::Id)
        .paginate(&db, pagination_query.page_size());

    let res = Paginated::from_paginator_query(paginator, pagination_query).await?;

    Ok(V1Response::success(res.items_into()))
}

async fn add_client(
    WithRejection(Json(payload), _): WithRejection<Json<ClientCreatePayload>, V1Error>,
) -> V1Result<ClientWithHidden> {
    let res = ClientService::create(&AppDb::db(), payload).await;

    let res = match res {
        Ok(res) => res,

        Err(e) => match &e {
            ClientCreateError::ClientAlreadyExists => {
                return Err(V1Response::error(
                    StatusCode::CONFLICT,
                    "Client with this name already exists",
                ));
            }
            _ => {
                return Err(V1Response::error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    e.to_string(),
                ));
            }
        },
    };

    Ok(V1Response::success(res))
}

async fn get_client(Path(client_uid): Path<String>) -> V1Result<ClientWithHidden> {
    let res = ClientService::get_by_api_key(&AppDb::db(), &client_uid).await?;
    let res = match res {
        Some(res) => res,
        None => return Err(V1Response::error(StatusCode::NOT_FOUND, "Client not found")),
    };
    Ok(V1Response::success(res))
}

async fn remove_client(Path(client_uid): Path<String>) -> V1Result<bool> {
    ClientService::delete_by_api_key(&AppDb::db(), client_uid).await?;
    Ok(V1Response::success(true))
}
