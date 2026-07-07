use axum::{
    body::Body,
    extract::Request,
    http::{Method, StatusCode},
    middleware::Next,
    response::Response,
};
use base64::Engine;

const HEADER_NAME: &str = "x-downloader-hub";
const GRACE_SECS: i64 = 10;

pub async fn require_timestamp_header(
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let method = req.method().clone();
    if !is_mutating(&method) {
        return Ok(next.run(req).await);
    }

    let raw = req
        .headers()
        .get(HEADER_NAME)
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::FORBIDDEN)?;

    let ts = decode_timestamp(raw).ok_or(StatusCode::FORBIDDEN)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs().cast_signed());
    if (now - ts).abs() > GRACE_SECS {
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(next.run(req).await)
}

const fn is_mutating(method: &Method) -> bool {
    matches!(
        method,
        &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE
    )
}

fn decode_timestamp(raw: &str) -> Option<i64> {
    let decoded = base64::engine::general_purpose::STANDARD.decode(raw).ok()?;
    let s = std::str::from_utf8(&decoded).ok()?;
    s.parse::<i64>().ok()
}
