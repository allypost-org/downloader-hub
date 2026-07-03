use app_database::Database;
use futures::StreamExt;
use tracing::{debug, warn};

use super::{SessionRegistry, sessions};

pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let registry: SessionRegistry = sessions().clone();
    let mut stream = Database::global().authed_watch_all().await?;

    debug!("Authed revocation watcher started");

    while let Some(emission) = stream.next().await {
        match emission {
            Ok(list) => {
                let valid = list
                    .iter()
                    .map(|entry| (entry.id.clone(), entry.expires_at))
                    .collect();
                let now_ms = u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or(0);
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
