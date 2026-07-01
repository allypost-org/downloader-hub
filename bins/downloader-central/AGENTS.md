# downloader-central

axum HTTP coordination server — the entrypoint peers connect to. See root `AGENTS.md` for toolchain/build commands.

## Layout

- `src/main.rs` — dotenv → `app_logger::init()` → `Config::init_parsed()` → `cmd::run`. Canonical boot sequence; copy this when adding a new binary.
- `src/cmd/central/components/` — spawns long-running services (HTTP server + peering handlers).
- `src/cmd/central/broadcaster.rs` — initial-state broadcaster to new peers.
- `src/cmd/central/auth/` — `/api/v1/join-ticket` and token issuance.
- `src/cmd/central/rpc_handler/` — incoming peer RPC.
- `src/cmd/central/config.rs` — clap `CentralConfig` (HTTP server + worker API + database).

## Startup contract (`async_run` in `src/cmd/central/mod.rs`)

1. `app_database::Database::init(config.database)` — connects to Convex.
2. `CentralConfig::init_jwt_secret(config.worker_api.jwt_secret)` — shared secret for worker/bot JWTs.
3. `Broadcaster::init()`.
4. `components::spawn(config)` — drives the runtime until a component task exits.

On shutdown, calls `PeeringEndpoint::global().router.shutdown()`.
