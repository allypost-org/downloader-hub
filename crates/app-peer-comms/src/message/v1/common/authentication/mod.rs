use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::jwt::JwtPair;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Authentication {
    ApiKey(Arc<str>),
    JwtPair(JwtPair),
    RefreshToken(Arc<str>),
}
