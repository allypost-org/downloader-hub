use std::{sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidAuth {
    pub authed_id: Arc<str>,
    pub expires_at: i64,
}

impl ValidAuth {
    pub fn until_expiry(&self) -> Duration {
        let now = chrono::Utc::now();
        let Some(expires) =
            chrono::DateTime::from_timestamp_secs(self.expires_at).map(|x| x.to_utc())
        else {
            return Duration::ZERO;
        };

        let diff = expires.signed_duration_since(now).abs();

        diff.to_std().expect("Should always be positive")
    }
}

#[derive(Debug)]
pub enum AuthError {
    MissingHeader,
    InvalidToken(String),
}
