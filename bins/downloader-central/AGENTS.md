# downloader-central

iroh coordination node — the entrypoint peers connect to over QUIC. Serves the irpc control protocol (`CentralProtocol`), publishes broadcasts on iroh-gossip, and runs a minimal axum HTTP surface (bootstrap/metrics/health). See root `AGENTS.md` for toolchain/build commands.

## Layout

- `src/main.rs` — dotenv → `app_logger::init()` → `Config::init_parsed()` → `cmd::run`. Canonical boot sequence; copy this when adding a new binary.
- `src/cmd/central/components/rpc/` — the irpc server. `CentralRpcServer` implements `iroh::protocol::ProtocolHandler` directly (per-connection auth state); `session.rs` is the connection-scoped session registry; `revocation.rs` is the Convex live-query watcher that tears down revoked/expired sessions; `distributor.rs` is the `WorkDistributor` actor that parks workers on `getWorkItem` and pre-takes an item (one the worker hasn't refused) the moment the Convex stream reports work. Auth is API-key-at-connect (the `Auth` call → `Database::authed_get_info_by_token`).
- `src/cmd/central/components/database/` — Convex watcher: feeds the `WorkDistributor` (`set_available`) on each `getAllAvailable` emission and runs the revocation watcher.
- `src/cmd/central/components/peers/` — subscribes to iroh-gossip (kept for neighbor presence / future use; inbound is drained — work distribution is via `getWorkItem`).
- `src/cmd/central/components/worker_api/` — minimal axum: `/api/v1/join-ticket` (HTTPS bootstrap — validates API key, returns the iroh ticket), `/api/v1/metrics`, `/health`.
- `src/cmd/central/config.rs` — clap `CentralConfig` (database + peer + worker-api/HTTP bind).

## Startup contract (`async_run` in `src/cmd/central/mod.rs`)

1. `app_database::Database::init(config.database)` — connects to Convex.
2. `components::spawn(config)`:
   - `rpc::init_sessions()` + `rpc::init_distributor()` — the auth-session registry and the work-distributor globals.
   - `init_peering(...)` — builds the `PeeringEndpoint`, registering the irpc `CentralRpcServer` on the iroh `Router` via `.with_router_hook(|b| b.accept(RPC_ALPN, CentralRpcServer::new()))`.
   - spawns the `Database`, `Peers`, and `Worker API` components (each in `keep_running`).

On shutdown, calls `PeeringEndpoint::global().router.shutdown()`.

There are no JWTs on central anymore — `/join-ticket` returns only the iroh ticket.
