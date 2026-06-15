use std::time::Duration;

use axum::{
    Router,
    http::{HeaderValue, Request, Response, header},
    routing::post,
};
use axum_client_ip::ClientIpSource;
use tower::ServiceBuilder;
use tower_http::{
    catch_panic::CatchPanicLayer,
    request_id::{MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    set_header::SetResponseHeaderLayer,
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use tracing::{Span, debug, field, info};

mod rpc;
mod v1;

pub struct RouterConfig {
    pub request_ip_source: ClientIpSource,
}

pub fn create_router(conf: &RouterConfig) -> Router {
    add_middlewares(
        conf,
        Router::new()
            .route("/api/rpc", post(rpc::post_rpc))
            .nest("/api/v1", v1::create_v1_router()),
    )
}

#[allow(clippy::too_many_lines)]
fn add_middlewares<T>(conf: &RouterConfig, router: Router<T>) -> Router<T>
where
    T: std::clone::Clone + Send + Sync + 'static,
{
    router.layer(CatchPanicLayer::new()).layer(
        ServiceBuilder::new()
            .layer(conf.request_ip_source.clone().into_extension())
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
                        info!(
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
            .layer(SetResponseHeaderLayer::if_not_present(
                header::CACHE_CONTROL,
                |_response: &Response<_>| {
                    Some("private, max-age=0".parse().expect("Invalid header value"))
                },
            ))
            .layer(SetResponseHeaderLayer::if_not_present(
                header::EXPIRES,
                |_response: &Response<_>| Some("-1".parse().expect("Invalid header value")),
            )),
    )
}

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
