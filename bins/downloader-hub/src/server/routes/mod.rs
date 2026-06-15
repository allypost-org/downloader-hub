use std::any::Any;

use axum::{
    Router,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tower_http::catch_panic::CatchPanicLayer;

use self::v1::response::V1Response;
use super::AppRouter;

mod index;
mod v1;

pub(super) fn router() -> AppRouter {
    Router::new()
        .nest("/", index::router())
        .nest("/v1", v1::router())
        .layer(CatchPanicLayer::custom(
            |err: Box<dyn Any + Send + 'static>| -> Response<_> {
                let details = err.downcast_ref::<String>().map_or_else(
                    || {
                        err.downcast_ref::<&str>().map_or_else(
                            || "Unknown panic message".to_string(),
                            |s| (*s).to_string(),
                        )
                    },
                    std::clone::Clone::clone,
                );

                V1Response::error(StatusCode::INTERNAL_SERVER_ERROR, details).into_response()
            },
        ))
}
