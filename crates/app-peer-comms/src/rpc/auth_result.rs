use serde::{Deserialize, Serialize};

use crate::rpc::session::AuthedInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthResult {
    Ok(AuthedInfo),
    Unauthorized,
}
