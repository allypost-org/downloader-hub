# Connection inventory + client heartbeats

## TL;DR

A new Convex table tracks which clients (workers/bots, with capabilities) are
connected to which central (by iroh NodeId). Clients send a `Heartbeat` RPC
every 30 s (jittered); central touches the row's `lastSeen`. A scheduled Convex
cleanup deletes rows not touched in 5 min. To let parked workers / streaming
bots send heartbeats concurrently, central's accept loop now spawns per-request
handlers (Auth stays inline).

## Decisions

- **Heartbeat**: client → central RPC (every 30 s + jitter).
- **Central identity**: iroh NodeId hex (stable via persisted `--peer-comms-secret-key`).
- **Capabilities**: typed `Capabilities { Worker{extractors}, Bot{platform} }`, stored in Convex as JSON (matches the existing opaque-`info` pattern).
- **Constants**: heartbeat 30 s (+jitter), TTL 5 min, cleanup cron 60 s.

## Design

### New table `downloader_hub_connections`
`{ central: string (NodeId hex), authed → id, role: "worker"|"bot", capabilities: optional string (JSON), lastSeen: int64(ms) }`.
Indexes: `by_central_authed` (`central`,`authed` — upsert), `by_last_seen` (`lastSeen` — TTL cleanup).

### Convex functions
- `connections:upsert(central, authed, role, capabilitiesJson)` — insert or update (`lastSeen=now`). Called on Auth.
- `connections:heartbeat(central, authed)` — touch `lastSeen=now` for `(central, authed)`.
- `connections:remove(central, authed)` — delete the row (on disconnect).
- `connections:list` — inventory (all rows).
- `connections:cleanup` — scheduled (every 60 s): delete `lastSeen < now − 5 min`.

### Protocol (`app-peer-comms/rpc`)
- `Capabilities { Worker { extractors: Vec<String> }, Bot { platform: String } }`.
- `Auth` gains `capabilities: Capabilities`.
- New `Heartbeat` method (oneshot → `()`).

### Central
- **Accept loop**: `Auth` processed inline (session established first); every later request is `tokio::spawn`'d so a parked `getWorkItem` / streaming watch doesn't block the heartbeat stream. Shared state is already thread-safe.
- On Auth: `connections:upsert(self_node_id, authed, role, capabilities)`.
- `Heartbeat` arm: `connections:heartbeat(self_node_id, authed)`.
- On connection close: `connections:remove(self_node_id, authed)` (plus unregister session).
- `self_node_id` cached in a `OnceLock<String>`.

### Client (worker + bot)
- `Auth` carries `Capabilities` (worker: enabled extractors via `available_extractors()`; bot: platform).
- A heartbeat task (spawned once via `OnceLock`) loops: sleep 30 s + jitter, call `RpcClient::heartbeat()`, log+swallow errors.

## Verification
- Send work; while a worker is parked, confirm `connections:list` shows it with `lastSeen` advancing every ~30 s.
- Kill a worker → its row disappears within ~5 min (TTL) or promptly if central observed the disconnect.
