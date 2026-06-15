use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub mod targeted;

pub(super) const MAIN_ISSUER: &str = "downloader-agent";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtPair {
    pub token: Arc<str>,
    pub refresh_token: Arc<str>,
}

impl JwtPair {
    #[must_use]
    pub const fn new(token: Arc<str>, refresh_token: Arc<str>) -> Self {
        Self {
            token,
            refresh_token,
        }
    }
}
