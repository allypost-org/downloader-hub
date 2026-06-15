use axum::{Extension, Router, middleware, routing::get};

use crate::server::{
    AppRouter,
    routes::v1::{
        middleware::auth::{CurrentUser, require_auth},
        response::V1Response,
    },
};

pub(super) fn router() -> AppRouter {
    Router::new()
        .route("/me/info", get(client_info))
        .route_layer(middleware::from_fn(require_auth))
}

async fn client_info(Extension(user): Extension<CurrentUser>) -> V1Response<CurrentUser> {
    V1Response::success(user)
}
