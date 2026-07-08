use app_database::Database;
use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::{IntoResponse, Response},
};
use futures::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::watch;
use tracing::{debug, warn};

use super::{AppState, auth::AdminSession, envelope::V1Response};

const RECENT_FAILED_LIMIT: i64 = 5;

#[derive(Clone)]
pub struct LiveSnapshots {
    counts: watch::Receiver<serde_json::Value>,
    recent_failed: watch::Receiver<serde_json::Value>,
    authed_names: watch::Receiver<serde_json::Value>,
    account_names: watch::Receiver<serde_json::Value>,
    /// Lightweight "request data changed" ping. Carries the latest
    /// `lastModified` timestamp; clients use it to invalidate their paginated
    /// request lists and refetch via HTTP rather than receiving the rows here.
    requests_changed: watch::Receiver<serde_json::Value>,
}

impl LiveSnapshots {
    pub fn spawn() -> Self {
        let (counts_tx, counts_rx) = watch::channel(serde_json::Value::Null);
        let (failed_tx, failed_rx) = watch::channel(serde_json::Value::Null);
        let (names_tx, names_rx) = watch::channel(serde_json::Value::Null);
        let (account_names_tx, account_names_rx) = watch::channel(serde_json::Value::Null);
        let (requests_changed_tx, requests_changed_rx) = watch::channel(serde_json::Value::Null);

        tokio::spawn(run_counts_watch(counts_tx));
        tokio::spawn(run_failed_watch(failed_tx));
        tokio::spawn(run_authed_names_watch(names_tx));
        tokio::spawn(run_account_names_watch(account_names_tx));
        tokio::spawn(run_requests_changed_watch(requests_changed_tx));

        Self {
            counts: counts_rx,
            recent_failed: failed_rx,
            authed_names: names_rx,
            account_names: account_names_rx,
            requests_changed: requests_changed_rx,
        }
    }
}

async fn run_counts_watch(tx: watch::Sender<serde_json::Value>) {
    let mut stream = match Database::global().requests_watch_counts().await {
        Ok(s) => s,
        Err(e) => {
            warn!(?e, "failed to start counts watch");
            return;
        }
    };
    debug!("started counts watch");
    while let Some(item) = stream.next().await {
        match item {
            Ok(counts) => {
                let value = serde_json::to_value(counts).unwrap_or(serde_json::Value::Null);
                let _ = tx.send(value);
            }
            Err(e) => warn!(?e, "counts watch error"),
        }
    }
    warn!("counts watch stream ended");
}

async fn run_failed_watch(tx: watch::Sender<serde_json::Value>) {
    // The recent-failed list watch used to use `requests_watch_by_status`,
    // which was removed when the list query became paginated. We now poll the
    // paginated query on a timer — fine for the dashboard's 5-row preview.
    debug!("started recent-failed watch (polled)");
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
    interval.tick().await; // immediate first tick
    loop {
        interval.tick().await;
        match Database::global()
            .requests_get_by_status(
                app_database::api::requests::RequestStatusType::Failed,
                Some(RECENT_FAILED_LIMIT),
                None,
            )
            .await
        {
            Ok(page) => {
                let value =
                    serde_json::to_value(page.page.as_ref()).unwrap_or(serde_json::Value::Null);
                let _ = tx.send(value);
            }
            Err(e) => warn!(?e, "recent-failed poll error"),
        }
    }
}

/// Pushes a `{ authedId -> name }` map whenever the `authed:listFull` result
/// changes. Watches the full list directly (Convex emits only on a genuine
/// result change), so there is no per-tick re-fetch. Identical maps are
/// deduplicated to avoid pushing no-op updates.
async fn run_authed_names_watch(tx: watch::Sender<serde_json::Value>) {
    let mut stream = match Database::global().authed_watch_full().await {
        Ok(s) => s,
        Err(e) => {
            warn!(?e, "failed to start authed-names watch");
            return;
        }
    };
    debug!("started authed-names watch");
    let mut last_signature: Option<String> = None;
    while let Some(item) = stream.next().await {
        match item {
            Ok(rows) => {
                let map: serde_json::Map<String, serde_json::Value> = rows
                    .iter()
                    .map(|a| {
                        (
                            a.id.as_ref().to_string(),
                            serde_json::Value::String(a.name.as_ref().to_string()),
                        )
                    })
                    .collect();
                let signature = serde_json::to_string(&map).unwrap_or_default();
                if last_signature.as_deref() == Some(signature.as_str()) {
                    continue;
                }
                last_signature = Some(signature);
                let value = serde_json::Value::Object(map);
                let _ = tx.send(value);
            }
            Err(e) => warn!(?e, "authed-names watch error"),
        }
    }
    warn!("authed-names watch stream ended");
}

/// Pushes a `{ users: { "<platform>:<id>": label }, places: { key: label } }`
/// map whenever the account metadata snapshot changes. Used by the SPA to
/// resolve `orderedBy`/`orderedIn` refs to display names.
async fn run_account_names_watch(tx: watch::Sender<serde_json::Value>) {
    let mut stream = match Database::global().accounts_watch_for_stream().await {
        Ok(s) => s,
        Err(e) => {
            warn!(?e, "failed to start account-names watch");
            return;
        }
    };
    debug!("started account-names watch");
    let mut last_signature: Option<String> = None;
    while let Some(item) = stream.next().await {
        match item {
            Ok(snapshot) => {
                let users_map: serde_json::Map<String, serde_json::Value> = snapshot
                    .users
                    .iter()
                    .map(|u| {
                        let key = format!("{}:{}", u.platform, u.platform_id);
                        let label = u
                            .display_name
                            .clone()
                            .or_else(|| u.username.clone())
                            .unwrap_or_else(|| u.platform_id.clone());
                        (key, serde_json::Value::String(label))
                    })
                    .collect();
                let places_map: serde_json::Map<String, serde_json::Value> = snapshot
                    .places
                    .iter()
                    .map(|p| {
                        let key = format!("{}:{}", p.platform, p.platform_id);
                        let label = p
                            .name
                            .clone()
                            .or_else(|| p.username.clone())
                            .unwrap_or_else(|| p.platform_id.clone());
                        (key, serde_json::Value::String(label))
                    })
                    .collect();
                let mut full = serde_json::Map::new();
                full.insert("users".into(), serde_json::Value::Object(users_map));
                full.insert("places".into(), serde_json::Value::Object(places_map));
                let signature = serde_json::to_string(&full).unwrap_or_default();
                if last_signature.as_deref() == Some(signature.as_str()) {
                    continue;
                }
                last_signature = Some(signature);
                let _ = tx.send(serde_json::Value::Object(full));
            }
            Err(e) => warn!(?e, "account-names watch error"),
        }
    }
    warn!("account-names watch stream ended");
}

/// Pushes a tiny ping whenever the latest `lastModified` among all requests
/// advances. The frontend uses this as a signal to invalidate its paginated
/// request lists and refetch via HTTP. Carries no row data.
///
/// Convex re-emits the watch on internal re-evaluations even when the value is
/// unchanged, so emissions are deduplicated on the `lastModified` value to
/// avoid hammering the client with refetch pings.
async fn run_requests_changed_watch(tx: watch::Sender<serde_json::Value>) {
    let mut stream = match Database::global().requests_watch_latest_change().await {
        Ok(s) => s,
        Err(e) => {
            warn!(?e, "failed to start requests-changed watch");
            return;
        }
    };
    debug!("started requests-changed watch");
    let mut last: Option<Option<u64>> = None;
    while let Some(item) = stream.next().await {
        match item {
            Ok(change) => {
                if last == Some(change.last_modified) {
                    continue;
                }
                last = Some(change.last_modified);
                let value = change.last_modified.map_or(serde_json::Value::Null, |ts| {
                    serde_json::to_value(ts).unwrap_or(serde_json::Value::Null)
                });
                let _ = tx.send(value);
            }
            Err(e) => warn!(?e, "requests-changed watch error"),
        }
    }
    warn!("requests-changed watch stream ended");
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
enum StreamMessage {
    Counts {
        data: serde_json::Value,
    },
    RecentFailed {
        data: serde_json::Value,
    },
    AuthedNames {
        data: serde_json::Value,
    },
    AccountNames {
        data: serde_json::Value,
    },
    /// `data` is the latest `lastModified` (u64 ms epoch) or null when no
    /// requests exist yet.
    RequestsChanged {
        data: serde_json::Value,
    },
}

#[allow(clippy::needless_pass_by_value)]
pub async fn ws_stream(
    _session: AdminSession,
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> Response {
    let Some(live) = state.live else {
        return V1Response::<()>::err(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "live snapshots not available",
        )
        .into_response();
    };

    ws.on_upgrade(move |socket| handle_stream_socket(socket, live))
}

async fn handle_stream_socket(socket: WebSocket, live: LiveSnapshots) {
    let (mut sender, mut receiver) = socket.split();

    let mut counts_rx = live.counts.clone();
    let mut failed_rx = live.recent_failed.clone();
    let mut names_rx = live.authed_names.clone();
    let mut account_names_rx = live.account_names.clone();
    let mut requests_changed_rx = live.requests_changed.clone();

    let _ = send_snapshot(&mut sender, &counts_rx, SnapshotKind::Counts).await;
    let _ = send_snapshot(&mut sender, &failed_rx, SnapshotKind::RecentFailed).await;
    let _ = send_snapshot(&mut sender, &names_rx, SnapshotKind::AuthedNames).await;
    let _ = send_snapshot(&mut sender, &account_names_rx, SnapshotKind::AccountNames).await;
    let _ = send_snapshot(
        &mut sender,
        &requests_changed_rx,
        SnapshotKind::RequestsChanged,
    )
    .await;

    loop {
        tokio::select! {
            result = counts_rx.changed() => match result {
                Ok(()) => {
                    if send_snapshot(&mut sender, &counts_rx, SnapshotKind::Counts).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            },
            result = failed_rx.changed() => match result {
                Ok(()) => {
                    if send_snapshot(&mut sender, &failed_rx, SnapshotKind::RecentFailed).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            },
            result = names_rx.changed() => match result {
                Ok(()) => {
                    if send_snapshot(&mut sender, &names_rx, SnapshotKind::AuthedNames).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            },
            result = account_names_rx.changed() => match result {
                Ok(()) => {
                    if send_snapshot(&mut sender, &account_names_rx, SnapshotKind::AccountNames).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            },
            result = requests_changed_rx.changed() => match result {
                Ok(()) => {
                    if send_snapshot(&mut sender, &requests_changed_rx, SnapshotKind::RequestsChanged).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            },
            msg = receiver.next() => match msg {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Err(e)) => {
                    debug!(?e, "ws recv error");
                    break;
                }
                _ => {}
            },
        }
    }
    debug!("ws stream socket closed");
}

enum SnapshotKind {
    Counts,
    RecentFailed,
    AuthedNames,
    AccountNames,
    RequestsChanged,
}

async fn send_snapshot(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    rx: &watch::Receiver<serde_json::Value>,
    kind: SnapshotKind,
) -> Result<(), axum::Error> {
    let data = rx.borrow().clone();
    // RequestsChanged legitimately emits null (no requests yet) — push it
    // anyway so the client sees the initial state. Other channels skip null
    // until their first real emission.
    let allow_null = matches!(kind, SnapshotKind::RequestsChanged);
    if !allow_null && data.is_null() {
        return Ok(());
    }
    let msg = match kind {
        SnapshotKind::Counts => StreamMessage::Counts { data },
        SnapshotKind::RecentFailed => StreamMessage::RecentFailed { data },
        SnapshotKind::AuthedNames => StreamMessage::AuthedNames { data },
        SnapshotKind::AccountNames => StreamMessage::AccountNames { data },
        SnapshotKind::RequestsChanged => StreamMessage::RequestsChanged { data },
    };
    let json = serde_json::to_string(&msg).unwrap_or_default();
    sender.send(Message::Text(json.into())).await
}
