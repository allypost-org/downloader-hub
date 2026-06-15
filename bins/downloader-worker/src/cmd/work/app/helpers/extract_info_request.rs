use app_actions::extractors::ExtractInfoRequest;
use app_peer_comms::message::v1::common::file::FileUrl;

pub fn file_url_to_extract_info_request(
    file_url: &FileUrl,
) -> Result<ExtractInfoRequest, Box<dyn std::error::Error + Send + Sync>> {
    let method: reqwest::Method = file_url.method.parse()?;
    let headers = file_url.headers.iter().filter_map(|(k, v)| {
        let k: reqwest::header::HeaderName = k.parse().ok()?;
        let v: reqwest::header::HeaderValue = v.parse().ok()?;

        Some((k, v))
    });

    Ok(
        app_actions::extractors::ExtractInfoRequest::new(file_url.url.clone())
            .with_method(method)
            .with_headers(headers),
    )
}
