use serde::{Deserialize, Serialize};

use crate::message::v1::central::work_request::WorkRequest;

/// One frame of a `WorkRequestWait` server stream.
///
/// `Unavailable` covers both non-owner and nonexistent requests so an
/// authenticated bot cannot use ids to discover another bot's requests.
/// `Overloaded` is sent when the per-authed (64) or global (512) watch capacity
/// is exhausted. `BackendError` ends the stream after a Convex failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::large_enum_variant)]
pub enum WorkRequestWatchEvent {
    Request(WorkRequest),
    Unavailable,
    Overloaded,
    BackendError,
}
