use std::borrow::Cow;

use url::Url;
use validator::ValidationError;

pub fn validate_is_absolute_url<'a, T>(url: T) -> Result<(), ValidationError>
where
    T: Into<Cow<'a, str>>,
{
    let parsed =
        Url::parse(url.into().as_ref()).map_err(|_| ValidationError::new("Invalid URL"))?;

    validate_url_is_absolute_url(&parsed)
}

pub fn validate_url_is_absolute_url(url: &Url) -> Result<(), ValidationError> {
    if url.cannot_be_a_base() {
        return Err(ValidationError::new("URL must be absolute"));
    }

    Ok(())
}

#[must_use]
pub fn value_parser_parse_absolute_url() -> impl clap::builder::TypedValueParser {
    move |s: &str| {
        let parsed = match Url::parse(s) {
            Ok(parsed) => parsed,
            Err(e) => return Err(format!("URL must be absolute: {e}")),
        };

        if parsed.cannot_be_a_base() {
            return Err("URL must be absolute".to_string());
        }

        Ok(parsed)
    }
}

#[must_use]
pub fn value_parser_parse_absolute_url_as_url() -> impl clap::builder::TypedValueParser {
    move |s: &str| {
        let parsed = match Url::parse(s) {
            Ok(parsed) => parsed,
            Err(e) => return Err(format!("URL must be absolute: {e}")),
        };

        if parsed.cannot_be_a_base() {
            return Err("URL must be absolute".to_string());
        }

        Ok(parsed)
    }
}
