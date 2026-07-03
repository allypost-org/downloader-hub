use std::fmt::Write;

use app_peer_comms::rpc::request::CapabilitiesSummary;
use tracing::warn;

#[derive(Debug, Clone, Copy)]
pub enum CapabilityKind {
    Extractors,
    Downloaders,
    Fixers,
}

/// Fetch the aggregate worker capabilities from central.
pub async fn fetch() -> Option<CapabilitiesSummary> {
    match crate::peering::rpc::RpcClient::get_capabilities().await {
        Ok(summary) => Some(summary),
        Err(e) => {
            warn!(?e, "Failed to fetch capabilities from central");
            None
        }
    }
}

/// Render one capability section as a plain-text bulleted list.
#[must_use]
pub fn render(kind: CapabilityKind, summary: &CapabilitiesSummary) -> String {
    let (title, entries) = match kind {
        CapabilityKind::Extractors => ("Extractors", &summary.extractors),
        CapabilityKind::Downloaders => ("Downloaders", &summary.downloaders),
        CapabilityKind::Fixers => ("Fixers", &summary.fixers),
    };
    if entries.is_empty() {
        return format!("{title}: none available (is a worker connected?).");
    }
    let mut out = format!("{title}:");
    for entry in entries {
        if entry.description.is_empty() {
            let _ = write!(out, "\n - {}", entry.name);
        } else {
            let _ = write!(out, "\n - {}: {}", entry.name, entry.description);
        }
    }
    out
}
