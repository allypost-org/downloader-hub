use clap::{Args, ValueHint};
use serde::{Deserialize, Serialize};
use url::Url;
use validator::Validate;

use crate::validators::url::{
    validate_url_is_absolute_url, value_parser_parse_absolute_url,
    value_parser_parse_absolute_url_as_url,
};

#[derive(derive_more::Debug, Clone, Serialize, Deserialize, Args, Validate)]
#[clap(next_help_heading = Some("External endpoints/APIs"))]
pub struct EndpointConfig {
    /// The base URL for the Twitter screenshot API.
    #[arg(long, default_value = "https://twitter.igr.ec", env = "DOWNLOADER_HUB_ENDPOINT_TWITTER_SCREENSHOT", value_hint = ValueHint::Url, value_parser = value_parser_parse_absolute_url())]
    #[validate(custom(function = "validate_url_is_absolute_url"))]
    pub twitter_screenshot_base_url: Url,

    /// The base URL for the OCR API.
    #[arg(long, env = "DOWNLOADER_HUB_ENDPOINT_OCR_API", value_hint = ValueHint::Url, value_parser = value_parser_parse_absolute_url_as_url())]
    #[debug("{:?}", ocr_api_base_url.as_ref().map(std::string::ToString::to_string))]
    pub ocr_api_base_url: Option<Url>,
}
impl EndpointConfig {
    #[must_use]
    pub fn ocr_api_url(&self, path: &str) -> Option<Url> {
        self.ocr_api_base_url
            .as_ref()
            .and_then(|x| x.join(path.trim_start_matches('/')).ok())
    }
}

impl Default for EndpointConfig {
    fn default() -> Self {
        Self {
            twitter_screenshot_base_url: Url::parse("https://twitter.igr.ec").expect("Invalid URL"),
            ocr_api_base_url: None,
        }
    }
}
