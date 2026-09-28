use std::{collections::HashMap, sync::Arc};

use app_database::{Database, api::requests::RequestCounts, error::ResponseError};
use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
};
use futures::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::broadcast;
use tracing::{debug, warn};

use super::{AppState, auth::AdminSession};

const BUS_CAPACITY: usize = 256;

type NamesMap = HashMap<Arc<str>, String>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountNamesPayload {
    pub users: NamesMap,
    pub places: NamesMap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LiveEvent {
    Counts(RequestCounts),
    AuthedNames { names: NamesMap },
    AccountNames(AccountNamesPayload),
    RequestsChanged,
    Resync,
}

#[derive(Clone)]
pub struct LiveBus {
    tx: broadcast::Sender<LiveEvent>,
}

impl LiveBus {
    #[must_use]
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BUS_CAPACITY);
        Self { tx }
    }

    pub fn publish(&self, event: LiveEvent) {
        let _ = self.tx.send(event);
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<LiveEvent> {
        self.tx.subscribe()
    }

    #[must_use]
    pub fn spawn_sources(db: &Arc<Database>) -> Self {
        let bus = Self::new();
        spawn_counts_source(db, &bus);
        spawn_authed_names_source(db, &bus);
        spawn_account_names_source(db, &bus);
        spawn_requests_changed_source(db, &bus);
        bus
    }
}

fn spawn_counts_source(db: &Arc<Database>, bus: &LiveBus) {
    let db = Arc::clone(db);
    let bus = bus.clone();
    tokio::spawn(async move {
        drive_source(
            "counts",
            db.requests_watch_counts().await,
            {
                let mut last: Option<RequestCounts> = None;
                move |counts| {
                    if last.as_ref() == Some(&counts) {
                        tracing::trace!(?last, "last `counts` didn't change");
                        return None;
                    }
                    last = Some(counts.clone());
                    Some(LiveEvent::Counts(counts))
                }
            },
            bus,
        )
        .await;
    });
}

fn spawn_authed_names_source(db: &Arc<Database>, bus: &LiveBus) {
    let db = Arc::clone(db);
    let bus = bus.clone();
    tokio::spawn(async move {
        drive_source(
            "authed-names",
            db.authed_watch_full().await,
            {
                let mut last: Option<NamesMap> = None;
                move |rows| {
                    let names: NamesMap = rows
                        .iter()
                        .map(|a| (a.id.clone(), a.name.to_string()))
                        .collect();
                    if last.as_ref() == Some(&names) {
                        tracing::trace!(?last, "last `authed-names` didn't change");
                        return None;
                    }
                    last = Some(names.clone());
                    Some(LiveEvent::AuthedNames { names })
                }
            },
            bus,
        )
        .await;
    });
}

fn spawn_account_names_source(db: &Arc<Database>, bus: &LiveBus) {
    let db = Arc::clone(db);
    let bus = bus.clone();
    tokio::spawn(async move {
        drive_source(
            "account-names",
            db.accounts_watch_for_stream().await,
            {
                let mut last: Option<AccountNamesPayload> = None;
                move |snapshot| {
                    let users = snapshot
                        .users
                        .iter()
                        .map(|u| {
                            let key = format!("{}:{}", u.platform, u.platform_id);
                            let label = u
                                .display_name
                                .clone()
                                .or_else(|| u.username.clone())
                                .unwrap_or_else(|| u.platform_id.clone());
                            (key.into(), label)
                        })
                        .collect::<NamesMap>();
                    let places = snapshot
                        .places
                        .iter()
                        .map(|p| {
                            let key = format!("{}:{}", p.platform, p.platform_id);
                            let label = p
                                .name
                                .clone()
                                .or_else(|| p.username.clone())
                                .unwrap_or_else(|| p.platform_id.clone());
                            (key.into(), label)
                        })
                        .collect::<NamesMap>();
                    let payload = AccountNamesPayload { users, places };
                    if last.as_ref() == Some(&payload) {
                        tracing::trace!(?last, "last `account-names` didn't change");
                        return None;
                    }
                    last = Some(payload.clone());
                    Some(LiveEvent::AccountNames(payload))
                }
            },
            bus,
        )
        .await;
    });
}

fn spawn_requests_changed_source(db: &Arc<Database>, bus: &LiveBus) {
    let db = Arc::clone(db);
    let bus = bus.clone();
    tokio::spawn(async move {
        drive_source(
            "requests-changed",
            db.requests_watch_latest_change().await,
            {
                let mut last: Option<Option<u64>> = None;
                move |change| {
                    if last == Some(change.last_modified) {
                        tracing::trace!(?last, "last `requests-changed` didn't change");
                        return None;
                    }
                    last = Some(change.last_modified);
                    Some(LiveEvent::RequestsChanged)
                }
            },
            bus,
        )
        .await;
    });
}

async fn drive_source<S, T, M>(
    name: &'static str,
    opened: Result<S, app_database::DatabaseError>,
    mut map: M,
    bus: LiveBus,
) where
    S: futures::Stream<Item = Result<T, ResponseError>> + Unpin,
    M: FnMut(T) -> Option<LiveEvent>,
{
    let mut stream = match opened {
        Ok(stream) => stream,
        Err(e) => {
            warn!(?e, name, "failed to start live source");
            return;
        }
    };
    let last_item = None;
    while let Some(item) = stream.next().await {
        match item {
            Ok(item) => {
                if let Some(event) = map(item) {
                    if last_item.as_ref() == Some(&event) {
                        tracing::trace!(?name, ?last_item, "last event didn't change");
                        continue;
                    }
                    bus.publish(event);
                }
            }
            Err(e) => warn!(?e, name, "live source error"),
        }
    }
    warn!(name, "live source stream ended");
}

#[allow(clippy::needless_pass_by_value)]
pub async fn ws_stream(
    _session: AdminSession,
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| handle_stream_socket(socket, state.bus))
}

async fn handle_stream_socket(socket: WebSocket, bus: LiveBus) {
    let (mut sender, mut receiver) = socket.split();
    let mut events = bus.subscribe();

    loop {
        tokio::select! {
            event = events.recv() => match event {
                Ok(event) => {
                    let json = serde_json::to_string(&event).unwrap_or_default();
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!(n, "ws stream client lagged; sending resync");
                    let json = serde_json::to_string(&LiveEvent::Resync).unwrap_or_default();
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
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
