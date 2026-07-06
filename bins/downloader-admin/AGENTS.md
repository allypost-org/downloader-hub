# downloader-admin

Operator UI for the hub — an axum server with an **embedded React 19 SPA** (`rust-embed`). Reads Convex directly and scrapes central over HTTP + irpc `Admin` role; performs privileged actions (retry/cancel/delete requests, manage authed tokens). Browser sessions are signed-cookie-authenticated against Convex `authed` rows with `for = "admin"`. See root `AGENTS.md` for toolchain/build commands and `docs/plans/2026-07-03_18-44_admin-bin-embedded-react-app.md` for the design.

## Layout

- `src/main.rs` — crypto provider → dotenv → `Config::init_parsed()` → `app_logger::init_with_options()` → `cmd::run`. Same canonical boot sequence as the other bins.
- `src/config/mod.rs` — crate-root `Config` (clap `Parser` + `GlobalConfig` + `Dumpable`); log fields + `cmd: CmdConfig`.
- `src/cmd/_config.rs` — `CmdConfig` enum (single `Run(AdminConfig)` variant) + manual `impl Validate`.
- `src/cmd/run/config.rs` — `AdminConfig`: `database` + `http` (`AdminHttpConfig`: bind host/port + `--admin-session-secret`) + `central: Option<PeerCommsAdminConfig>` (central URL + admin API key; `None` ⇒ DB-only mode).
- `src/cmd/run/components/mod.rs` — `init(central_cfg)` fetches central's join ticket (admin target), builds the `PeeringEndpoint`, dials central, sends irpc `Auth` with `Capabilities::Admin`. Returns `Option<Arc<CentralClient>>` (None on failure → HTTP API degrades to DB-only).
- `src/cmd/run/components/central/mod.rs` — `CentralClient`: wraps the irpc `Client<CentralProtocol>`; exposes `list_sessions`/`list_parked_workers` (admin RPCs) and HTTP proxies to central's `/connections` + `/metrics` via `app_requests::Client`.
- `src/cmd/run/components/http_api/mod.rs` — the axum router: `/api/admin/*` JSON API + embedded SPA (`/`, `/assets/*`, deep-link fallback to `index.html`). `AppState` holds the `SessionKey` + optional `CentralClient`. `keep_running`-supervised.
- `src/cmd/run/components/http_api/auth.rs` — signed-cookie sessions (`axum_extra::extract::SignedCookieJar`, HMAC via `cookie::Key::derive_from(session_secret)`). `AdminSession` extractor rejects requests without a valid cookie. Cookie claims = `{admin_id, exp}`, 12h sliding.
- `src/cmd/run/components/http_api/routes.rs` — handlers: `auth/login`/`logout`/`me`, `requests` (list by status, get, retry, cancel, remove, clear-refusals), `connections`, `metrics`, `authed` (list/create/revoke/rotate/remove), `central/sessions` + `central/parked-workers`.
- `src/cmd/run/components/http_api/envelope.rs` — `V1Response<T>` JSON envelope (`{status, data}` / `{status, error}`), copied from central.
- `src/cmd/run/components/http_api/spa.rs` — `rust-embed` of `frontend/dist`; serves assets by path, falls back to `index.html`.
- `frontend/` — the Vite SPA (see below).

## Startup contract (`async_run` in `src/cmd/run/mod.rs`)

1. `app_database::Database::init(config.database)` — connects to Convex.
2. Creates an `Arc<ArcSwapOption<CentralClient>>` (shared with HTTP routes) and spawns two components:
   - `HTTP API` (`keep_running`-supervised): binds `--admin-http-host`:`--admin-http-port` (default `0.0.0.0:8082`) **immediately**, serving DB-only until the central client populates the slot.
   - `Central client` (one-shot task): `components::connect_central` fetches the join ticket via `run_retried` (retries transient failures — central down, network blip), builds the `PeeringEndpoint`, irpc `Auth` as Admin, and stores the `CentralClient` into the slot. Exits permanently on auth-reject, missing config, or exhausted retries — it does **not** restart on a clean exit (so no auth-fail hammering).

On shutdown, calls `PeeringEndpoint::global().router.shutdown()` (only if a central connection was established).

## Frontend (`frontend/`)

React 19 + Vite + `@tanstack/react-router` + `@tanstack/react-query` + zustand + TailwindCSS v4 + ShadCN-style components. The SPA **is rebuilt automatically** by `build.rs`: if any file under `frontend/src/` (or `index.html`/`package.json`/`bun.lock`/vite/ts config) is newer than `frontend/dist/index.html`, the build script runs `bun install` (if `node_modules` is missing) then `bun run build` before compiling the Rust crate. Set `SKIP_FRONTEND_BUILD=1` to skip (e.g. for a fast `cargo check` when you know `dist` is fine, or if `bun` isn't available — in that case it embeds whatever's already in `dist`). Dev workflow: `bun run dev` (Vite dev server on 5173 with `/api` proxied to the Rust bin on 8082) — the `admin-fe` mprocs entry.

Boot: `App.tsx` fetches `/auth/me` once; on 401 the router renders `/login`. The `_authed` layout route guards all pages (redirects to `/login` when no session). Login posts an admin token to `/auth/login`; the bin validates it against Convex and sets the signed cookie.

## Auth model

- **Browser → bin**: signed http-only cookie (HMAC-SHA256 of `{admin_id, readonly, exp}`, 12h sliding), `SameSite=Strict`, `Secure` in release builds (plaintext HTTP allowed in debug builds). The login POST is the only place a raw token crosses the wire. The SPA additionally sends an `X-Downloader-Hub: <base64 unix-secs>` header on every mutating request; the backend rejects it if missing or more than ~10s off current time — this is the CSRF defense (cookies-only auth + custom header).
- **Read-only admins**: an `authed` row with `readonly = true` and `for = "admin"` logs in but cannot mutate — every mutating route additionally requires a non-readonly session (`WriteSession` extractor → 403). The UI surfaces this with a "read-only" badge and disables mutating buttons.
- **Bin → central**: a dedicated server-side admin token from `--admin-central-api-key`/`DOWNLOADER_HUB_PEER_COMMS_ADMIN_API_KEY` (independent of whoever is logged in). Validates via `/join-ticket` + irpc `Auth`; central's dispatch gates the admin RPCs on `is_admin`. The connection is supervised: on a drop, a liveness probe (periodic `GetCapabilities`) detects it and the bin re-dials central automatically. It only stops retrying on a missing config or an `Unauthorized` auth response (so a bad key never hammers central).

## Env / dev

Required env (via `.env`, picked up by `just`): `DOWNLOADER_HUB_DATABASE_URL`, `DOWNLOADER_HUB_ADMIN_SESSION_SECRET` (**required**, ≥32 bytes), and (for central-backed panels) `DOWNLOADER_HUB_PEER_COMMS_ADMIN_API_URL` + `DOWNLOADER_HUB_PEER_COMMS_ADMIN_API_KEY`. To provision an admin token: insert a row into Convex `downloader_hub_authed` with `for = "admin"` (or use the `authed:create` mutation once the bin is running).
