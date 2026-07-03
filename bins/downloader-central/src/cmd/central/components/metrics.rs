use std::{
    fmt::Write,
    sync::atomic::{AtomicU64, Ordering},
};

static SESSIONS_ACTIVE: AtomicU64 = AtomicU64::new(0);
static PARKED_WORKERS: AtomicU64 = AtomicU64::new(0);
static AUTH_OK: AtomicU64 = AtomicU64::new(0);
static AUTH_UNAUTHORIZED: AtomicU64 = AtomicU64::new(0);
static WORK_ITEMS_DISPATCHED: AtomicU64 = AtomicU64::new(0);
static RPC_REQUESTS: AtomicU64 = AtomicU64::new(0);

pub fn session_added() {
    SESSIONS_ACTIVE.fetch_add(1, Ordering::Relaxed);
}

pub fn session_removed(n: u64) {
    SESSIONS_ACTIVE.fetch_sub(n, Ordering::Relaxed);
}

pub fn set_parked_workers(n: usize) {
    PARKED_WORKERS.store(u64::try_from(n).unwrap_or(u64::MAX), Ordering::Relaxed);
}

pub fn auth_ok() {
    AUTH_OK.fetch_add(1, Ordering::Relaxed);
}

pub fn auth_unauthorized() {
    AUTH_UNAUTHORIZED.fetch_add(1, Ordering::Relaxed);
}

pub fn work_item_dispatched() {
    WORK_ITEMS_DISPATCHED.fetch_add(1, Ordering::Relaxed);
}

pub fn rpc_request() {
    RPC_REQUESTS.fetch_add(1, Ordering::Relaxed);
}

/// Render all metrics in Prometheus text-exposition format.
#[must_use]
pub fn render() -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "# HELP downloader_hub_sessions_active Currently authenticated irpc sessions."
    );
    let _ = writeln!(out, "# TYPE downloader_hub_sessions_active gauge");
    let _ = writeln!(
        out,
        "downloader_hub_sessions_active {}",
        SESSIONS_ACTIVE.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "# HELP downloader_hub_parked_workers Workers parked on getWorkItem."
    );
    let _ = writeln!(out, "# TYPE downloader_hub_parked_workers gauge");
    let _ = writeln!(
        out,
        "downloader_hub_parked_workers {}",
        PARKED_WORKERS.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "# HELP downloader_hub_auth_total irpc Auth calls by result."
    );
    let _ = writeln!(out, "# TYPE downloader_hub_auth_total counter");
    let _ = writeln!(
        out,
        "downloader_hub_auth_total{{result=\"ok\"}} {}",
        AUTH_OK.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "downloader_hub_auth_total{{result=\"unauthorized\"}} {}",
        AUTH_UNAUTHORIZED.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "# HELP downloader_hub_work_items_dispatched_total Work items handed to workers."
    );
    let _ = writeln!(
        out,
        "# TYPE downloader_hub_work_items_dispatched_total counter"
    );
    let _ = writeln!(
        out,
        "downloader_hub_work_items_dispatched_total {}",
        WORK_ITEMS_DISPATCHED.load(Ordering::Relaxed)
    );
    let _ = writeln!(
        out,
        "# HELP downloader_hub_rpc_requests_total irpc requests handled (all methods)."
    );
    let _ = writeln!(out, "# TYPE downloader_hub_rpc_requests_total counter");
    let _ = writeln!(
        out,
        "downloader_hub_rpc_requests_total {}",
        RPC_REQUESTS.load(Ordering::Relaxed)
    );
    out
}
