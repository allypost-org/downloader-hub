use axum::{
    Json,
    extract::Path,
    http::{StatusCode, header, header::CONTENT_TYPE},
    response::{Html, IntoResponse, Response},
};
use rust_embed::RustEmbed;
use serde_json::json;

#[derive(RustEmbed)]
#[folder = "frontend/dist"]
struct SpaAssets;

pub async fn serve_asset(Path(path): Path<String>) -> Response {
    let key = format!("assets/{path}");
    if let Some(file) = SpaAssets::get(&key) {
        let mime = mime_guess::from_path(&key).first_or_octet_stream();
        return (StatusCode::OK, [(CONTENT_TYPE, mime.as_ref())], file.data).into_response();
    }
    index_html().await
}

pub async fn index_html() -> Response {
    if let Some(file) = SpaAssets::get("index.html") {
        return Html(file.data).into_response();
    }
    let body = json!({
        "status": "error",
        "error": "Admin frontend not built. Run `bun run build` in bins/downloader-admin/frontend.",
    });
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "application/json")],
        Json(body),
    )
        .into_response()
}
