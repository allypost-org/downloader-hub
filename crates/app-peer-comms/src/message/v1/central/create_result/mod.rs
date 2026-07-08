use std::sync::Arc;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CreateResult {
    Ok(CreateResultData),
    BackendError,
    Unauthorized,
    Banned { reason: String },
    RateLimited { retry_after: jiff::Span },
}

impl CreateResult {
    /// Human-readable message for the [`RateLimited`] variant, rounded to whole
    /// seconds (e.g. "1min", "59s", "1min 5s"). Returns `None` for other
    /// variants.
    ///
    /// [`RateLimited`]: CreateResult::RateLimited
    #[must_use]
    pub fn rate_limit_message(&self) -> Option<String> {
        let Self::RateLimited { retry_after } = self else {
            return None;
        };
        let rounded = retry_after
            .round(jiff::Unit::Second)
            .unwrap_or(*retry_after);
        let text = jiff::fmt::friendly::SpanPrinter::new().span_to_string(&rounded);
        Some(format!("Rate limit exceeded. Try again in {text}."))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateResultData {
    pub id: Arc<str>,
}
