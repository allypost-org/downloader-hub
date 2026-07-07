use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use app_peer_comms::{
    IrohConnection as Connection,
    rpc::{request::AdminSessionInfo, session::Role},
};

use crate::cmd::central::components::metrics;

#[derive(Clone, Default)]
pub struct SessionRegistry {
    inner: Arc<Mutex<RegistryInner>>,
}

impl std::fmt::Debug for SessionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionRegistry").finish_non_exhaustive()
    }
}

#[derive(Default)]
struct RegistryInner {
    next_id: u64,
    by_id: HashMap<u64, Session>,
    by_authed: HashMap<Arc<str>, HashSet<u64>>,
}

struct Session {
    authed_id: Arc<str>,
    conn: Connection,
    role: Role,
    connected_at: u64,
    expires_at: Option<u64>,
}

impl SessionRegistry {
    pub fn register(
        &self,
        authed_id: Arc<str>,
        conn: Connection,
        role: Role,
        expires_at: Option<u64>,
    ) -> u64 {
        let connected_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0));
        let id = {
            let mut inner = self.inner.lock().expect("session registry poisoned");
            let id = inner.next_id;
            inner.next_id += 1;
            inner.by_id.insert(
                id,
                Session {
                    authed_id: authed_id.clone(),
                    conn,
                    role,
                    connected_at,
                    expires_at,
                },
            );
            inner.by_authed.entry(authed_id).or_default().insert(id);
            id
        };
        metrics::session_added();
        id
    }

    pub fn list(&self) -> Vec<AdminSessionInfo> {
        let inner = self.inner.lock().expect("session registry poisoned");
        inner
            .by_id
            .values()
            .map(|s| AdminSessionInfo {
                authed_id: s.authed_id.clone(),
                role: s.role.clone(),
                connected_at: s.connected_at,
                expires_at: s.expires_at,
            })
            .collect()
    }

    pub fn unregister(&self, id: u64) {
        let mut inner = self.inner.lock().expect("session registry poisoned");
        if let Some(session) = inner.by_id.remove(&id) {
            remove_authed_index(&mut inner, &session.authed_id, id);
            metrics::session_removed(1);
        }
    }

    #[allow(clippy::needless_collect, clippy::significant_drop_tightening)]
    pub fn revoke_invalid(&self, valid: &HashMap<Arc<str>, Option<u64>>, now_ms: u64) -> usize {
        let closed = {
            let mut inner = self.inner.lock().expect("session registry poisoned");
            let invalid: Vec<u64> = inner
                .by_id
                .iter()
                .filter(|(_, session)| {
                    let latest_expiry = valid.get(&session.authed_id).copied().flatten();
                    let effective_expiry = latest_expiry.or(session.expires_at);
                    !valid.contains_key(&session.authed_id)
                        || effective_expiry.is_some_and(|t| t < now_ms)
                })
                .map(|(&id, _)| id)
                .collect();

            invalid
                .into_iter()
                .filter_map(|id| {
                    let session = inner.by_id.remove(&id)?;
                    remove_authed_index(&mut inner, &session.authed_id, id);
                    Some(session.conn)
                })
                .collect::<Vec<_>>()
        };

        let n = closed.len();
        for conn in closed {
            conn.close(1u32.into(), b"session revoked");
        }
        metrics::session_removed(u64::try_from(n).unwrap_or(u64::MAX));
        n
    }
}

fn remove_authed_index(inner: &mut RegistryInner, authed_id: &Arc<str>, id: u64) {
    if let Some(set) = inner.by_authed.get_mut(authed_id) {
        set.remove(&id);
        if set.is_empty() {
            inner.by_authed.remove(authed_id);
        }
    }
}
