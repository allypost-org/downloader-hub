use std::sync::Arc;

use arc_swap::ArcSwapOption;
use axum::{
    Router,
    middleware::from_fn,
    routing::{delete, get, patch, post},
};
use tower_http::catch_panic::CatchPanicLayer;
use tracing::{debug, error, info};

use super::ComponentResult;
use crate::cmd::run::{components::central::CentralClient, config::AdminHttpConfig};

pub mod auth;
pub mod csrf;
pub mod envelope;
pub mod routes;
pub mod spa;
pub mod stream;

#[derive(Clone)]
pub struct AppState {
    pub session_key: Arc<auth::SessionKey>,
    pub central: Arc<ArcSwapOption<CentralClient>>,
    pub live: Option<stream::LiveSnapshots>,
}

impl AppState {
    #[must_use]
    pub fn central(&self) -> Option<Arc<CentralClient>> {
        self.central.load_full()
    }
}

pub async fn run(
    config: AdminHttpConfig,
    central: Arc<ArcSwapOption<CentralClient>>,
    session_secret: Arc<str>,
) -> ComponentResult {
    let session_key = Arc::new(auth::SessionKey::new(session_secret.as_bytes()));
    info!("starting live snapshots");
    let live = stream::LiveSnapshots::spawn();
    let state = AppState {
        session_key: session_key.clone(),
        central,
        live: Some(live),
    };

    let app = create_router(state);

    let listener = match tokio::net::TcpListener::bind(config.bind_addr()).await {
        Ok(l) => l,
        Err(e) => {
            error!(?e, addr = %config.bind_addr(), "Failed to bind admin HTTP server");
            return Err(e.into());
        }
    };
    info!(addr = %config.bind_addr(), "Admin HTTP server listening");

    axum::serve(listener, app.into_make_service())
        .await
        .map_err(|e| {
            error!(?e, "Admin HTTP server failed");
            super::ComponentError::from(e)
        })?;

    debug!("Admin HTTP server exited");
    Ok(())
}

fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(spa::index_html))
        .route("/assets/{*path}", get(spa::serve_asset))
        .route("/{*path}", get(spa::index_html))
        .nest("/api/admin", api_router())
        .layer(CatchPanicLayer::new())
        .with_state(state)
}

fn api_router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(routes::login))
        .route("/auth/logout", post(routes::logout))
        .route("/auth/me", get(routes::me))
        .route("/stream", get(stream::ws_stream))
        .route("/requests", get(routes::list_requests))
        .route("/requests/counts", get(routes::list_counts))
        .route(
            "/requests/{id}",
            get(routes::get_request).delete(routes::remove_request),
        )
        .route("/requests/{id}/retry", post(routes::retry_request))
        .route("/requests/{id}/cancel", post(routes::cancel_request))
        .route(
            "/requests/{id}/clear-refusals",
            post(routes::clear_refusals),
        )
        .route("/connections", get(routes::connections))
        .route("/metrics", get(routes::metrics))
        .route(
            "/authed",
            get(routes::list_authed).post(routes::create_authed),
        )
        .route("/authed/{id}", delete(routes::remove_authed))
        .route("/authed/{id}/revoke", post(routes::revoke_authed))
        .route("/authed/{id}/rotate", post(routes::rotate_authed))
        .route("/central/sessions", get(routes::central_sessions))
        .route(
            "/central/parked-workers",
            get(routes::central_parked_workers),
        )
        .route("/accounts/users", get(routes::list_account_users))
        .route("/accounts/places", get(routes::list_account_places))
        .route("/accounts/users/{id}", patch(routes::update_account_user))
        .route("/accounts/places/{id}", patch(routes::update_account_place))
        .route(
            "/requests/backfill-ordered-refs",
            post(routes::backfill_ordered_refs),
        )
        .route(
            "/restrictions",
            get(routes::list_restrictions).post(routes::create_restriction),
        )
        .route(
            "/restrictions/{id}",
            delete(routes::remove_restriction).put(routes::replace_restriction),
        )
        .layer(from_fn(csrf::require_timestamp_header))
}
