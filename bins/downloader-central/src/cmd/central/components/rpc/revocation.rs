use futures::StreamExt;
use tracing::{debug, warn};

use super::session::SessionRegistry;
use crate::cmd::central::components::state::SharedCentralState;

pub async fn run(
    state: SharedCentralState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let registry: SessionRegistry = state.sessions.clone();
    let mut stream = state.db().authed_watch_all().await?;

    debug!("Authed revocation watcher started");

    while let Some(emission) = stream.next().await {
        match emission {
            Ok(list) => {
                let valid = list
                    .iter()
                    .map(|entry| (entry.id.clone(), entry.expires_at))
                    .collect();
                let now_ms = u64::try_from(jiff::Timestamp::now().as_millisecond()).unwrap_or(0);
                let closed = registry.revoke_invalid(&valid, now_ms);
                if closed > 0 {
                    warn!(closed, "Closed revoked/expired irpc sessions");
                }
            }
            Err(e) => warn!(?e, "Authed watch emission error"),
        }
    }

    warn!("Authed revocation watcher stream ended");
    Ok(())
}
