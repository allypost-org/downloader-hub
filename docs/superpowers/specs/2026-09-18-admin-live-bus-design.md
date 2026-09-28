# Admin live-stream: app-wide message bus

Date: 2026-09-18
Status: implemented

## Problem

The admin live-stream (`bins/downloader-admin/src/cmd/run/components/http_api/stream.rs`)
grew one hand-written pipeline per feed: five watch tasks, five
`watch::Sender<serde_json::Value>` channels, five identical `select!` arms, two
dedup strategies (serialize-to-string signatures, `Option` compare), and an
`allow_null` special case. Adding a feed touches ~8 places, and the data is
untyped until the wire boundary. The chat-facing bot status messages are
explicitly out of scope (their verbosity is a feature).

## Inspiration

`emailager-mvp`'s notification system: one typed event enum, one broadcast hub,
emit-side noise policy, publish failure never fails the business action, and a
`resync` marker that tells a lagging consumer to re-read state instead of
replaying events.

## Design

### One bus

`LiveBus` wraps a single `tokio::broadcast::Sender<LiveEvent>` (capacity 256)
and lives on `AppState` (replacing `LiveSnapshots`).

- Sources publish typed events; sockets subscribe. Publish failure (no
  receivers) is ignored - normal idle state, never an error.
- Every connected admin socket receives every event. Per-socket filtering, if
  ever needed, is one `matches!` in the forward loop.

### One event enum

```rust
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LiveEvent {
    Counts(RequestCounts),                     // {"type":"counts","pending":"1",...}
    AuthedNames { names: NamesMap },           // {"type":"authedNames","names":{...}}
    AccountNames(AccountNamesPayload),         // {"type":"accountNames","users":{...},"places":{...}}
    RequestsChanged,                           // {"type":"requestsChanged"}
    Resync,                                    // {"type":"resync"}
}
```

- Payloads are concrete types (`RequestCounts` from app-database; local
  `NamesMap = HashMap<Arc<str>, String>` maps), `Serialize + PartialEq + Clone`.
  No `serde_json::Value` anywhere.
- `RequestsChanged` carries no data; the backend dedups on `lastModified`
  internally, the client just invalidates request queries.
- Wire format is redesigned (stop-the-world deploy acceptable): flattened
  `{"type", ...}` shape, no `data` wrapper.

### Sources: one generic pump

A single pump consumes any DB watch stream:

```rust
async fn run_pump<S, T, M>(name, source, map: M, bus)
// source: Future<Output = Result<Stream<T>, DatabaseError>>
// map: FnMut(T) -> Option<LiveEvent>  (None = no change; owns dedup state)
// stream error -> warn and continue; stream end -> warn and exit
```

Four sources, each a `spawn_*_source` task plus a `*_mapper` constructor
(the mapper owns the dedup state and returns `None` for no-change):

1. `requests_watch_counts` -> `Counts` (dedup on `RequestCounts: PartialEq`)
2. `authed_watch_full` -> `AuthedNames` (id -> name map, dedup on map equality)
3. `accounts_watch_for_stream` -> `AccountNames` (label maps, dedup on equality)
4. `requests_watch_latest_change` -> `RequestsChanged` (dedup on `lastModified`)

The recent-failed feed is **deleted**: it was a 5s poll whose only effect
(pushing the dashboard's failed list) is already covered by `RequestsChanged`
invalidating `["requests"]`, which refetches that list over HTTP.

### Socket handler

Subscribe to the bus, forward events as JSON lines, on
`RecvError::Lagged(n)` send `Resync` and continue, on `Closed`/client close
break. No snapshot-on-connect: on socket open the client invalidates its live
query prefixes and refetches over HTTP (the queries already have `queryFn`s
with polling fallbacks).

### Frontend (`useLiveStream.ts`)

Typed discriminated union mirroring `LiveEvent`, defensive `parseEvent`
(unparseable / unknown `type` -> `null`), and an exhaustive `switch` with an
`assertNever` default so a new backend event breaks the build until handled.
`onopen` performs the same invalidations as `resync`.

## Consequences

- New live feed = one enum variant + one pump spawn site + one switch arm.
- Added `PartialEq, Eq` to `RequestCounts` (app-database) for structural dedup.
- Initial dashboard paint comes from HTTP (same as today's `queryFn` path);
  the socket only accelerates updates and signals changes.
