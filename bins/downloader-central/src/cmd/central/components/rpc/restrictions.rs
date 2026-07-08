use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};

use app_database::entity::{
    accounts::{AccountPlaceRef, AccountUserRef},
    restrictions::{RestrictionRow, Rule},
};
use arc_swap::ArcSwap;
use futures::StreamExt;
use tracing::warn;

use super::restrictions;

/// In-memory mirror of the `restrictions` table plus the token buckets for
/// every active `Limit` rule. Lock-free reads via `ArcSwap`; the token-bucket
/// state lives behind a `Mutex` held only for microseconds (no `await` inside).
#[derive(Clone)]
pub struct RestrictionsManager {
    rows: Arc<ArcSwap<Vec<RestrictionRow>>>,
    buckets: Arc<Mutex<HashMap<Arc<str>, Bucket>>>,
}

struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

/// Decision returned by [`RestrictionsManager::check`].
pub enum AdmitDecision {
    Allow,
    Banned { reason: String },
    RateLimited { retry_after: jiff::Span },
}

impl RestrictionsManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            rows: Arc::new(ArcSwap::from_pointee(Vec::<RestrictionRow>::new())),
            buckets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Replace the mirrored restriction rows and reconcile the token buckets:
    /// new `Limit` rules start full at `count`; buckets for removed rows are dropped.
    pub fn update(&self, rows: &Arc<[RestrictionRow]>) {
        {
            let mut buckets = self
                .buckets
                .lock()
                .expect("restrictions buckets lock poisoned");
            let live: HashMap<&Arc<str>, u64> = rows
                .iter()
                .filter_map(|row| match &row.rule {
                    Rule::Limit { count, .. } => Some((&row.id, *count)),
                    Rule::Ban { .. } => None,
                })
                .collect();
            buckets.retain(|id, _| live.contains_key(id));
            for (id, count) in live {
                buckets.entry(id.clone()).or_insert_with(|| Bucket {
                    tokens: count_f64(count),
                    last_refill: Instant::now(),
                });
            }
        }
        self.rows.store(rows.to_vec().into());
    }

    /// Check whether a request from `(user, place)` should be admitted.
    /// Consumes one token from every matching `Limit` bucket iff the result is
    /// `Allow`; never consumes on a denial. Synchronous — no `await` inside.
    #[must_use]
    pub fn check(
        &self,
        user: Option<&AccountUserRef>,
        place: Option<&AccountPlaceRef>,
    ) -> AdmitDecision {
        let rows = self.rows.load_full();
        let now = jiff::Timestamp::now();

        for row in rows.iter() {
            if !matches_scope(row.user.as_ref(), user, row.place.as_ref(), place) {
                continue;
            }
            if let Rule::Ban { reason, ends_at } = &row.rule {
                let active = ends_at.is_none_or(|t| t > now);
                if active {
                    return AdmitDecision::Banned {
                        reason: reason.clone(),
                    };
                }
            }
        }

        let matching_limits: Vec<(&Arc<str>, u64, jiff::Span)> = rows
            .iter()
            .filter_map(|row| {
                if !matches_scope(row.user.as_ref(), user, row.place.as_ref(), place) {
                    return None;
                }
                match &row.rule {
                    Rule::Limit { count, timeframe } => Some((&row.id, *count, *timeframe)),
                    Rule::Ban { .. } => None,
                }
            })
            .collect();
        if matching_limits.is_empty() {
            return AdmitDecision::Allow;
        }

        let mut buckets = self
            .buckets
            .lock()
            .expect("restrictions buckets lock poisoned");
        let now_instant = Instant::now();

        let mut worst_retry_ms: i64 = 0;
        for (id, count, timeframe) in &matching_limits {
            let count_f64 = count_f64(*count);
            let bucket = buckets.entry((*id).clone()).or_insert_with(|| Bucket {
                tokens: count_f64,
                last_refill: now_instant,
            });
            let window_ms = timeframe
                .total(jiff::Unit::Millisecond)
                .map_or(1.0, |f| f.max(1.0));
            let rate = count_f64 / window_ms;
            let elapsed_ms = millis_f64(now_instant.duration_since(bucket.last_refill));
            bucket.tokens = count_f64.min(elapsed_ms.mul_add(rate, bucket.tokens));
            bucket.last_refill = now_instant;

            if bucket.tokens < 1.0 {
                let need = 1.0 - bucket.tokens;
                #[allow(clippy::cast_possible_truncation)]
                let retry_ms = (need / rate).ceil() as i64;
                if retry_ms > worst_retry_ms {
                    worst_retry_ms = retry_ms;
                }
            }
        }

        if worst_retry_ms > 0 {
            return AdmitDecision::RateLimited {
                retry_after: jiff::Span::new()
                    .try_milliseconds(worst_retry_ms.max(1))
                    .unwrap_or_default(),
            };
        }

        for (id, _, _) in &matching_limits {
            if let Some(bucket) = buckets.get_mut(&***id) {
                bucket.tokens -= 1.0;
            }
        }
        AdmitDecision::Allow
    }
}

#[allow(clippy::cast_precision_loss)]
const fn millis_f64(d: std::time::Duration) -> f64 {
    d.as_millis() as f64
}

#[allow(clippy::cast_precision_loss)]
const fn count_f64(n: u64) -> f64 {
    n as f64
}

impl Default for RestrictionsManager {
    fn default() -> Self {
        Self::new()
    }
}

/// `(row.user == none || row.user == req.user) && (row.place == none || row.place == req.place)`
fn matches_scope(
    row_user: Option<&AccountUserRef>,
    req_user: Option<&AccountUserRef>,
    row_place: Option<&AccountPlaceRef>,
    req_place: Option<&AccountPlaceRef>,
) -> bool {
    let user_ok = row_user.is_none_or(|ru| req_user.is_some_and(|qu| qu == ru));
    let place_ok = row_place.is_none_or(|rp| req_place.is_some_and(|qp| qp == rp));
    user_ok && place_ok
}

pub async fn run() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mgr = restrictions().expect("restrictions manager not initialized");
    let mut stream = app_database::Database::global()
        .restrictions_watch_all()
        .await?;

    tracing::debug!("Restrictions watcher started");

    while let Some(emission) = stream.next().await {
        match emission {
            Ok(rows) => mgr.update(&rows),
            Err(e) => warn!(?e, "Restrictions watch emission error"),
        }
    }

    warn!("Restrictions watch stream ended");
    Ok(())
}
