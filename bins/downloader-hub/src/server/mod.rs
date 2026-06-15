use std::{net::SocketAddr, sync::LazyLock, time::Duration};

use axum::{
    Extension,
    http::{HeaderValue, Request, header},
    middleware,
    response::Response,
};
use listenfd::ListenFd;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::{
    catch_panic::CatchPanicLayer,
    cors::{self, CorsLayer},
    request_id::{MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    set_header::SetResponseHeaderLayer,
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use tracing::{Span, debug, field, info, trace};

use self::app_middleware::auth::CurrentUser;
use crate::{config::Config, db::AppDb};

pub mod app_helpers;
mod app_middleware;
mod app_response;
mod routes;

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting server...");
    let state = AppState::new().await?;
    trace!(state = ?state, "Created app state");

    let router = routes::router();
    let router = add_middlewares(router).with_state(state);

    trace!(?router, "Finished building app router");

    let mut listenfd = ListenFd::from_env();
    let listener = match listenfd.take_tcp_listener(0)? {
        Some(listener) => TcpListener::from_std(listener).expect("Failed to create listener"),
        None => {
            let server_config = &Config::server().run;
            let host = server_config.host.clone();
            let port = server_config.port;

            TcpListener::bind((host, port))
                .await
                .expect("Failed to create listener")
        }
    };

    info!("Server listening on http://{}", listener.local_addr()?);

    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

static CACHE_CONTROL: LazyLock<HeaderValue> =
    LazyLock::new(|| HeaderValue::from_static("private, max-age=0"));

#[derive(Clone)]
struct MakeRequestUlid;
impl MakeRequestId for MakeRequestUlid {
    fn make_request_id<B>(&mut self, _request: &Request<B>) -> Option<RequestId> {
        let mut id = ulid::Ulid::new().to_string();
        id.make_ascii_lowercase();
        let val = HeaderValue::from_str(&id).ok()?;

        Some(RequestId::new(val))
    }
}

type AppRouter = axum::Router<AppState>;

#[derive(Debug, Clone)]
struct AppState {
    pub db: AppDb,
}

impl AppState {
    #[allow(clippy::unused_async)]
    async fn new() -> anyhow::Result<Self> {
        Ok(Self {
            db: AppDb::global(),
        })
    }
}

fn add_middlewares<T>(router: axum::Router<T>) -> axum::Router<T>
where
    T: std::clone::Clone + Send + Sync + 'static,
{
    router
        .layer(CatchPanicLayer::new())
        .layer(
            ServiceBuilder::new()
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUlid))
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(|request: &Request<_>| {
                            let m = request.method();
                            let p = request.uri().path();
                            let id = request
                                .extensions()
                                .get::<RequestId>()
                                .and_then(|id| id.header_value().to_str().ok())
                                .unwrap_or("-");
                            let dur = field::Empty;
                            let user = field::Empty;

                            tracing::info_span!("", %id, %m, ?p, dur, user)
                        })
                        .on_request(|request: &Request<_>, _span: &Span| {
                            let headers = request.headers();
                            info!(
                                target: "request",
                                "START \"{method} {uri} {http_type:?}\" {user_agent:?} {ip:?}",
                                http_type = request.version(),
                                method = request.method(),
                                uri = request.uri(),
                                user_agent = headers
                                    .get(header::USER_AGENT)
                                    .map_or("-", |x| x.to_str().unwrap_or("-")),
                                ip = headers
                                    .get("x-forwarded-for")
                                    .map_or("-", |x| x.to_str().unwrap_or("-")),
                            );
                        })
                        .on_response(|response: &Response<_>, latency, span: &Span| {
                            span.record("dur", field::debug(latency));
                            debug!(
                                target: "request",
                                "END {status}",
                                status = response.status().as_u16(),
                            );
                        })
                        .on_body_chunk(())
                        .on_eos(|_trailers: Option<&_>, stream_duration, span: &Span| {
                            span.record("dur", field::debug(stream_duration));
                            debug!(
                                target: "request",
                                "ERR: stream closed unexpectedly",
                            );
                        })
                        .on_failure(|error, latency, span: &Span| {
                            span.record("dur", field::debug(latency));
                            debug!(
                                target: "request",
                                err = ?error,
                                "ERR: something went wrong",
                            );
                        }),
                )
                .layer(TimeoutLayer::new(Duration::from_mins(1)))
                .layer(PropagateRequestIdLayer::x_request_id())
                .layer(SetResponseHeaderLayer::if_not_present(
                    header::CACHE_CONTROL,
                    |_response: &Response<_>| Some(CACHE_CONTROL.clone()),
                ))
                .layer(SetResponseHeaderLayer::appending(
                    header::DATE,
                    |_response: &Response<_>| {
                        Some(
                            chrono::Utc::now()
                                .to_rfc2822()
                                .parse()
                                .expect("Invalid date"),
                        )
                    },
                ))
                .layer(middleware::map_response(
                    |current_user: Option<Extension<CurrentUser>>, mut res: Response<_>| async {
                        if let Some(Extension(current_user)) = current_user {
                            let headers = res.headers_mut();
                            if let Ok(val) = HeaderValue::from_str(&current_user.name) {
                                headers.insert("x-client-key-name", val);
                            }
                        }

                        res
                    },
                )),
        )
        .layer(
            CorsLayer::new()
                .allow_methods(cors::AllowMethods::mirror_request())
                .allow_origin(cors::AllowOrigin::mirror_request()),
        )
}
