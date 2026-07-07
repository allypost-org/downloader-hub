# Admin bin: `downloader-admin` (embedded React 19 SPA)

## TL;DR

A new 5th binary, `downloader-admin`, that monitors system state and performs
privileged actions (retry failed/done tasks, cancel/delete requests, clear
refusals, manage authed tokens). It is a separate bin to segregate deploy
concerns and responsibilities from the worker/central/bot runtime. It reads
live state from **two** sources:

1. **Convex directly** (`app-database` client) — for all persisted state
   (requests, authed tokens, connection inventory) and for the privileged
   mutations.
2. **`downloader-central`** — via the existing HTTPS surface
   (`/api/v1/connections`, `/api/v1/metrics`) **and** a new **irpc `Admin`
   role** with two read-only RPCs (`AdminListSessions`, `AdminListParkedWorkers`)
   for in-process state central does not persist.

The UI is a **React 19 SPA** built with Vite, tanstack router/query, zustand,
tailwindcss, ShadCN. The built assets are embedded into the Rust binary with
`rust-embed` and served by the same axum router that serves the JSON API.

Auth reuses the existing Convex `authed` table: an `admin` role is added to the
`for` union; users log in with an admin token, the bin validates it via the
existing `authed_get_info_by_token` query, and sets a **signed http-only
cookie** (HMAC-SHA256, 12h sliding).

---

## Background & Context

### Current process architecture (4 binaries)

Per root `AGENTS.md`: `downloader-central` (iroh coordination node + minimal
axum), `downloader-worker` (downloads), `downloader-bot` (Telegram/Discord),
`downloader-cli` (local tool). State lives in Convex; central is the only binary
that writes Convex for task state today (workers/bots route through central via
irpc). There is no operator UI — ops inspect the system via logs, the Convex
dashboard, and `GET /api/v1/metrics`.

### Why an admin tool now

- Retrying failed tasks currently requires editing Convex directly (no mutation
  transitions `failed → pending`; only `inProgress → pending` exists in
  `free`/`release`/`refuse`).
- No way to list failed/done tasks from the Rust client (queries are
  status-scoped to `pending`/`inProgress`).
- No way to create/revoke worker & bot tokens (no `authed` mutations exist at
  all — tokens are seeded out-of-band).
- Live node/capabilities view exists only as JSON on `/api/v1/connections`;
  parked-worker count only as a Prometheus counter.

### What already exists we can reuse

- **`app-database`** transports any Convex function by name
  (`DatabaseRequest::named(...).query/mutate(self)`); adding new functions needs
  no transport change, just a typed wrapper in `src/api/`.
- **`connections_list()`** + `authed_watch_all()` / `authed_get_info_by_*` cover
  the read side of nodes/tokens today.
- **Central axum surface** (`/api/v1/join-ticket`, `/connections`, `/metrics`,
  `/health`) with a `V1Response<T>` JSON envelope and Bearer-auth helper — copy
  the pattern.
- **irpc + irpc-iroh** over the existing `PeeringEndpoint`; clients connect with
  `irpc_iroh::client::<CentralProtocol>(endpoint, ticket.main, RPC_ALPN)` and
  `Auth` as the first call (see `crates/app-peer-comms/src/rpc/mod.rs`).
- **`by_status_type` index** on `requests` (`status.Type`, `requester`) already
  supports filtered `failed`/`done` queries with no schema index change.
- **`requests_clear_refusals`** already resets `tries`/`refusedBy` (no status
  change) — reused verbatim as one of the v1 actions.

### What does not exist and must be added

- `admin` value in `authed.for` / `connections.role` unions (schema).
- `Capabilities::Admin` / `Role::Admin` / `TicketTarget::Admin` (irpc).
- Convex queries: `requests:getByStatus`. Convex mutations: `requests:retry`,
  `requests:cancel`, `requests:delete`. Convex authed functions: `listFull`,
  `create`, `revoke`, `rotateToken`, `remove`.
- The bin itself.

---

## Design Decisions (with rationale)

### D1: Separate bin, not a feature of central

**Decision:** Admin is a new `bins/downloader-admin` binary, deployed
independently (its own Docker image, its own deploy cadence), not a route on
central.

**Rationale (user-chosen):**
- Segregates responsibilities: central stays minimal (iroh + tiny axum); admin
  carries the heavy frontend + auth surface.
- An admin redeploy (UI change, new panel) does not roll central and disrupt the
  live mesh.
- Failure isolation: a panic in admin request handling can't take down
  coordination.
- Matches the existing 4-bin model (one concern per binary).

**Trade-off:** Two processes to run where there could be one. Mitigated by the
existing `mprocs`/Docker setup which already handles N binaries uniformly.

### D2: Read central live state via **both** HTTPS + a new irpc `Admin` role

**Decision:** The admin bin talks to central over **two** channels:

1. **HTTPS** to central's existing `/api/v1/connections` (Bearer) and
   `/api/v1/metrics` (no auth) — reused as-is, zero central HTTP change beyond
   accepting `admin` tokens on `/join-ticket`.
2. **irpc** as a new `Admin` role, with two new **read-only** `CentralProtocol`
   variants: `AdminListSessions` (bound sessions: authed_id, role, connected_at)
   and `AdminListParkedWorkers` (workers parked on `WorkDistributor`).

Plus **direct Convex reads** (`requests_get_by_status`, `authed_list_full`,
`connections_list`) for everything that's persisted.

**Rationale (user-chosen, "Both"):**
- HTTP/DB covers ~95% of monitoring with no protocol change.
- The irpc Admin role gives authoritative in-process data (active sessions,
  parked workers) that exists only in central's memory — these have no other
  source of truth.
- An admin role on the wire makes admin access explicit and revocable, rather
  than impersonating worker/bot.

**Trade-off:** New `CentralProtocol` variants touch central's wire surface.
Mitigation: read-only, gated `if !is_admin { Unauthorized }`; positional-postcard
constraint (D9 of the irpc plan) respected — no `skip`/`skip_serializing_if` on
new wire types, field counts matched on both sides, round-trip smoke-tested.

### D3: Auth reuses Convex `authed` tokens with a new `admin` role; signed-cookie browser sessions

**Decision:**
- An `admin` value is added to `authed.for` (and `connections.role`). Operators
  provision admin tokens the same way they provision worker/bot tokens (a new
  `authed:create` mutation — see D6).
- **Browser login**: user submits their admin token on a login page → the bin
  calls `authed_get_info_by_token(token)`, requires `for_role == Admin` → on
  success sets an **http-only** cookie containing `{admin_id, exp}` HMAC-SHA256
  signed with a `--admin-session-secret`.
- **Session lifetime**: 12h, **sliding** (each authenticated request refreshes
  `exp`).
- **Logout**: clears the cookie.

**Rationale (user-chosen):**
- Reuses the existing identity model (no second auth system). An admin token is
  just another row in `authed`.
- Signed cookie = stateless server-side; no session table, no shared state
  across admin instances.
- http-only + SameSite cookies are the browser-native, CSRF-resistant default.
- Sliding 12h is enough for an internal tool and bounds exposure of a stolen
  cookie.

**Trade-off:** Revoking a logged-in user's browser access requires (a) revoking
their Convex token (which kills the bin→central link but not the cookie until
exp) and (b) waiting out or rotating `--admin-session-secret`. Acceptable for an
internal tool; mitigation: keep 12h exp; rotate secret on offboarding.

### D4: The bin's own central connection uses a dedicated server-side admin token

**Decision:** The bin holds **one** admin token in its config/env
(`--admin-central-api-key`, role `admin`) that it uses for: the irpc `Auth` to
central, `/join-ticket`, `/connections`, `/metrics`. This is **independent** of
whoever is logged into the UI.

**Rationale (user-chosen):**
- Browser sessions are short-lived and per-user; the bin→central QUIC connection
  should be long-lived and shared. Decoupling avoids re-dialing central on every
  login/logout.
- Central's connection inventory shows the bin as one stable admin peer, not a
  churn of per-user sessions.

**Trade-off:** The bin holds a privileged credential. Mitigation: env/config
only (gitignored `.env`), same model as the existing worker/bot server-side
tokens; rotate via the new `authed:rotateToken`.

### D5: Retry works from `failed` **and** `done`; cancel and delete both supported

**Decision:**
- **`requests:retry`** transitions `failed` **or** `done` → `pending`
  (`tries:0n, errors:[], refusedBy:[], lastModified:now`); cancels any leftover
  `CleanupId`. Central's existing `available_work_watcher` then auto-redistributes.
- **`requests:cancel`** terminal-fails `pending`/`inProgress` →
  `{Type:"failed", reason:"Cancelled by admin"}`; no-op on terminal rows.
  History-preserving (row stays queryable as failed).
- **`requests:delete`** hard `ctx.db.delete` on any status. Irreversible; UI
  requires explicit confirm.

**Rationale (user-chosen):**
- Retry from `done` lets ops re-run a download that completed but produced a bad
  file.
- Two distinct destructive verbs (`cancel` vs `delete`) match operator mental
  models: "stop this and keep the record" vs "remove it entirely."

**Trade-off:** `retry` from `done`/`failed` skips the `inProgress` ownership
checks the existing transitions enforce. Acceptable because admin is itself the
privileged actor (and the row is not currently owned by any worker).

### D6: Authed token management — `create`/`revoke`/`rotateToken`/`remove`, tokens never listed

**Decision:** New Convex mutations in `authed.ts`:
- `authed:create({ name, for, readonly, onlyTagged?, expiresAt? })` — inserts
  with a freshly generated random token; returns `{ id, token }`.
- `authed:rotateToken({ id })` — replaces the token; returns the new one.
- `authed:revoke({ id })` — soft revoke (`expiresAt = now`).
- `authed:remove({ id })` — hard delete.
- `authed:listFull` query — returns all rows **without** the `token` field
  (tokens are write-once-read-once from the UI; never returned in list).

**Rationale:** No `authed` mutations exist today (tokens are seeded out-of-band
somehow). The UI needs CRUD. Tokens are never listed to limit blast radius of a
leaked list view; a forgotten token is recovered by rotation, not retrieval.

**Trade-off:** Losing a token means rotating it (mild inconvenience). Stronger
default for an admin surface.

### D7: Frontend stack — Vite + React 19 + tanstack router/query + zustand + tailwind + ShadCN

**Decision (user-specified):** The SPA lives under
`bins/downloader-admin/frontend/`. Vite builds to `dist/`; tanstack router
(file-based) for routing; tanstack query for server state; zustand for client
state (auth store); tailwindcss for styling; ShadCN for component primitives.

**Rationale:** User request. All mainstream, actively-maintained, type-safe.

### D8: SPA embedding — `rust-embed` + `build.rs` auto-build

**Decision (revised during implementation; originally "Always manual"):**
- `rust-embed` embeds `frontend/dist` into the binary at compile time.
- A `build.rs` rebuilds the SPA automatically: if any file under `frontend/src/`
  (or `index.html`/`package.json`/`bun.lock`/vite/ts config) is newer than
  `frontend/dist/index.html`, it runs `bun install` (if `node_modules` is
  missing) then `bun run build` before compiling the Rust crate. Set
  `SKIP_FRONTEND_BUILD=1` to skip (e.g. for a fast `cargo check`, or when `bun`
  isn't available — in which case it embeds whatever is already in `dist`).
- A non-`/api` GET falls through to `index.html` (tanstack router handles
  client routing).
- If `dist` is empty/missing and the build is skipped, the bin still compiles
  and serves the JSON API, but the SPA route 404s.

**Rationale (revised):** The original "manual `bun run build`" plan was flipped
in favour of a `build.rs` so `cargo build`/`just build` always produces a binary
with an up-to-date frontend, removing the footgun where a release image ships a
stale SPA. The `SKIP_FRONTEND_BUILD=1` escape hatch preserves the fast-iterate
workflow. Dev workflow still uses the Vite dev server (HMR) with `/api` proxied
to the running Rust bin; the `admin-fe` mprocs entry runs it.

**Trade-off:** `cargo build` is no longer side-effect-free: a frontend TS error
fails the whole workspace build via `build.rs`. Mitigated by the escape hatch
and by the Docker stage running `bun run build` explicitly with
`SKIP_FRONTEND_BUILD=1` on the `cargo build` step.

### D9: Privileged mutations enforced server-side; cookie is the only client credential

**Decision:** Every `/api/admin/*` route (except `/auth/login`) requires a valid
signed cookie. The cookie identifies the admin `authed_id`; the **bin's**
server-side admin token (D4) performs the actual Convex mutations / central RPCs
— the browser never sees or sends a token after login.

**Rationale:** Limits token exposure to the single login POST. All subsequent
requests are bearer-less (cookie only). Central/Convex see the bin's identity,
not the end-user's — consistent with D4.

**Trade-off:** Audit log of "which admin did what" must be reconstructed from
the admin bin's request logs (cookie → admin_id), not from Convex `by` fields
(which will record the bin's token). Acceptable for v1.

---

## Design

### Data model changes (Convex)

**`schema.ts`** — extend two unions (no new tables, no new indexes):

```ts
authed.for        : v.union(v.literal("worker"), v.literal("bot"), v.literal("admin"))
connections.role  : v.union(v.literal("worker"), v.literal("bot"), v.literal("admin"))
```

### New Convex functions

**`requests.ts`** (mirroring existing `ok`/`code` result-union style):

| Function | Kind | Args | Returns | Notes |
| --- | --- | --- | --- | --- |
| `getByStatus` | query | `statusType: "pending"\|"inProgress"\|"done"\|"failed"`, `limit?: int64` | `array(requestDataReturn)` | uses `by_status_type`, `.take(limit ?? 100)` |
| `retry` | mutation | `requestId` | `Ok \| RequestNotFound \| RequestNotRetryable` | `failed`\|`done` → `pending`, reset tries/errors/refusedBy, cancel `CleanupId` |
| `cancel` | mutation | `requestId, by` | `Ok \| RequestNotFound` | `pending`\|`inProgress` → `failed("Cancelled by admin")`, cancel cleanup |
| `delete` | mutation | `requestId` | `Ok \| RequestNotFound` | hard `ctx.db.delete` |

**`authed.ts`** (no authed mutations exist today — all new):

| Function | Kind | Args | Returns |
| --- | --- | --- | --- |
| `listFull` | query | — | `array({ id, name, for, readonly, onlyTagged, expiresAt })` (**no `token`**) |
| `create` | mutation | `name, for, readonly, onlyTagged?, expiresAt?` | `{ id, token }` |
| `rotateToken` | mutation | `id` | `{ token }` |
| `revoke` | mutation | `id` | `Ok \| NotFound` (sets `expiresAt = now`) |
| `remove` | mutation | `id` | `Ok \| NotFound` |

Token generation: `crypto.random` (Convex runtime), encoded as a URL-safe base64
~32-byte string, prefixed like existing tokens if a prefix convention exists.

### Rust DB client wrappers (`crates/app-database/src/api/`)

Add to `requests/mod.rs`, mirroring the `DatabaseRequest::named(...).query/mutate`
pattern: `requests_get_by_status`, `requests_retry`, `requests_cancel`,
`requests_delete`. Add `authed/mod.rs` (new file) with `authed_list_full`,
`authed_create`, `authed_rotate_token`, `authed_revoke`, `authed_remove` +
response structs (all without `token` except the create/rotate responses).

Also extend `entity/authed/mod.rs::AuthedForRole` with `Admin` + its `From`/`Display` impls.

### irpc `Admin` role + central RPCs (`crates/app-peer-comms/src/rpc/`)

- `ticket/targeted.rs::TicketTarget` — add `Admin` (`short() -> "admn"`).
- `rpc/request.rs::Capabilities` — add `Admin { }` variant (unit, or optional
  `name` for nicer logging).
- `rpc/session.rs::Role` — add `Admin`.
- `rpc/mod.rs::CentralProtocol` — add (positional-postcard, no `skip_*`):
  - `#[rpc(tx = oneshot::Sender<AdminSessionsResult>)]    AdminListSessions(request::AdminListSessions)`
  - `#[rpc(tx = oneshot::Sender<AdminParkedWorkersResult>)] AdminListParkedWorkers(request::AdminParkedWorkers)`
- New request/response types in `rpc/request.rs`: `AdminListSessions`,
  `AdminListParkedWorkers`, `AdminSessionInfo { authed_id, role, connected_at }`,
  `AdminParkedWorker { authed_id, since }`.

**Central** (`bins/downloader-central`):
- `routes/v1/root.rs::get_join_ticket` — `AuthedForRole::Admin => TicketTarget::Admin`.
- `components/rpc/mod.rs` accept loop — accept `Capabilities::Admin`, bind
  `Role::Admin`.
- `components/rpc/mod.rs::dispatch` — `is_admin`; handle the two new variants,
  gated, sourced from `SessionRegistry` (sessions) and `WorkDistributor`
  (parked workers), both already in-process. `GetCapabilities` stays ungated.

### `downloader-admin` bin layout

Mirrors central's structure (central's `AGENTS.md` calls its `main.rs` "the
canonical boot sequence; copy this when adding a new binary"):

```
bins/downloader-admin/
├── Cargo.toml
├── AGENTS.md
├── frontend/                      # entire Vite SPA (Phase 5)
│   ├── package.json
│   ├── vite.config.ts             # dev server 5173, proxy /api → 127.0.0.1:8082
│   ├── tsconfig.json
│   ├── tailwind.config.ts / postcss.config.js / components.json
│   ├── index.html
│   └── src/
│       ├── main.tsx
│       ├── routes/                # tanstack file-based: _authed.tsx, login.tsx,
│       │                          #   _authed/index.tsx (dashboard),
│       │                          #   _authed/requests/, _authed/requests.$id.tsx,
│       │                          #   _authed/nodes.tsx, _authed/tokens.tsx,
│       │                          #   _authed/metrics.tsx
│       ├── lib/{api.ts, auth.ts}
│       ├── stores/auth-store.ts   # zustand
│       ├── components/ui/         # ShadCN
│       └── components/            # StatusBadge, RequestTable, RequestDetail,
│                                  #   ConnectionList, TokenManager, MetricsView,
│                                  #   ConfirmAction
└── src/
    ├── main.rs                    # canonical boot (crypto→dotenv→Config→logger→cmd::run)
    ├── config/mod.rs              # Config (GlobalConfig + Dumpable) + log fields + cmd
    └── cmd/
        ├── _config.rs             # CmdConfig (single `run` subcommand)
        ├── mod.rs                 # run() -> run_future(async_run)
        └── run/
            ├── mod.rs             # async_run: Database::init + components::spawn
            ├── config.rs          # AdminConfig (database + http + central client + session secret)
            └── components/
                ├── mod.rs         # spawn -> JoinSet (keep_running pattern)
                ├── http_api/      # axum: /api/admin/* + embedded SPA + auth middleware
                │   ├── mod.rs
                │   ├── routes/{auth,requests,connections,metrics,authed,central}.rs
                │   ├── auth.rs    # signed-cookie session middleware (HMAC-SHA256)
                │   ├── envelope.rs # V1Response<T> (copied from central)
                │   └── spa.rs     # rust-embed assets + index.html fallback
                └── central/       # central client: HTTP scraper + irpc Admin client
                    ├── mod.rs
                    ├── http.rs    # reqwest → central /connections, /metrics
                    └── rpc.rs     # irpc Auth(Capabilities::Admin) + Admin* RPC wrappers
```

### Boot sequence (`main.rs`)

Byte-for-byte the central skeleton (the canonical one): install crypto provider
→ dotenvy → `Config::init_parsed()` → `app_logger::init_with_options(...)` →
`cmd::run(config.cmd)`.

### `async_run`

1. `Database::init(config.database).await?`.
2. If central config present: fetch `/join-ticket` (server-side admin token),
   build `PeeringEndpoint`, `irpc_iroh::client::<CentralProtocol>(...)`, send
   `Auth` with `Capabilities::Admin`. Store the `RpcClient` in a `OnceLock`.
   (Mirror `bins/downloader-worker/src/cmd/work/rpc.rs`.)
3. `components::spawn` → axum HTTP component.
4. Shutdown: `PeeringEndpoint::get_global().router.shutdown()`.

### HTTP API (all under `/api/admin`, JSON via `V1Response<T>` envelope)

| Method | Path | Purpose |
| --- | --- | --- |
| POST | `/auth/login` `{ token }` | validate admin token, set signed cookie |
| POST | `/auth/logout` | clear cookie |
| GET | `/auth/me` | current admin identity |
| GET | `/requests?status=&limit=` | `requests_get_by_status` |
| GET | `/requests/:id` | `requests_get` |
| POST | `/requests/:id/retry` | `requests_retry` |
| POST | `/requests/:id/cancel` | `requests_cancel` |
| DELETE | `/requests/:id` | `requests_delete` |
| POST | `/requests/:id/clear-refusals` | `requests_clear_refusals` (existing) |
| GET | `/connections` | central `/api/v1/connections` (fallback `connections_list`) |
| GET | `/metrics` | central `/api/v1/metrics` (Prometheus text) |
| GET | `/authed` | `authed_list_full` |
| POST | `/authed` | `authed_create` (returns token once) |
| POST | `/authed/:id/revoke` | `authed_revoke` |
| POST | `/authed/:id/rotate` | `authed_rotate_token` |
| DELETE | `/authed/:id` | `authed_remove` |
| GET | `/central/sessions` | irpc `AdminListSessions` |
| GET | `/central/parked-workers` | irpc `AdminListParkedWorkers` |

Central-backed routes degrade gracefully to DB-only (or 503) when the central
client isn't connected.

### Config (clap `Args`, flattening shared `common::*` blocks)

- `database: DatabaseConfig` (`DOWNLOADER_HUB_DATABASE_URL`).
- `--admin-http-host` / `--admin-http-port` (default `0.0.0.0:8082`).
- `--admin-session-secret` (HMAC key for cookie signing).
- `--admin-central-api-url` + `--admin-central-api-key` (server-side admin
  token; optional → central-backed panels degrade to DB-only).
- Log fields (`log_file`, `log_format`, `log_file_format`) + `dump` as in the
  other bins.

### Frontend pages

- **Login** — token input → `POST /auth/login`.
- **Dashboard** — KPIs (pending/inProgress/failed counts), recent failures,
  live connections, central metrics summary.
- **Requests** — filterable table (status tabs), per-row actions (Retry, Cancel,
  Clear refusals, Delete w/ confirm). Detail view: `info`, `errors[]`, status
  history, `tries`, `refusedBy`.
- **Nodes** — connection inventory + central sessions/parked workers.
- **Tokens** — list (masked), create (token shown once), revoke, rotate, delete.
- **Metrics** — rendered Prometheus text.

Realtime via react-query `refetchInterval` (5–15s polling) for v1.

---

## Implementation Plan

> Conventions (per root `AGENTS.md`): `just fmt-dev` for all lint/fix (never raw
> `cargo fmt`/`clippy`). Build-check per step with `just dev-build <package>`.
> No `cargo test` (no test suite). No comments/docs except clap help text.

### Phase 0 — `admin` role in schema + entity

1. `crates/app-database/convex/schema.ts` — add `admin` to both unions.
2. `crates/app-database/src/entity/authed/mod.rs` — add `AuthedForRole::Admin`
   + `From`/`Display` impls.
3. `just db-dev` (regenerate `_generated/`), `bun run check`, `just dev-build
   app-database`.

### Phase 1 — Convex backend functions

4. `convex/requests.ts` — add `getByStatus`, `retry`, `cancel`, `delete`.
5. `convex/authed.ts` — add `listFull`, `create`, `rotateToken`, `revoke`,
   `remove`.
6. `bun run check` (typecheck + prettier).

### Phase 2 — Rust DB client wrappers

7. `crates/app-database/src/api/requests/mod.rs` — add the 4 typed wrappers +
   result enums.
8. `crates/app-database/src/api/authed/mod.rs` (new) — add the 5 typed wrappers
   + response structs.
9. `just dev-build app-database`, `just fmt-dev`.

### Phase 3 — irpc Admin role + central RPCs

10. `crates/app-peer-comms/src/ticket/targeted.rs` — `TicketTarget::Admin`.
11. `crates/app-peer-comms/src/rpc/{request,session,mod}.rs` — `Capabilities::Admin`,
    `Role::Admin`, the two new `CentralProtocol` variants + wire types.
12. `bins/downloader-central` — join-ticket arm, accept loop, dispatch arms.
13. `just dev-build app-peer-comms downloader-central`, `just fmt-dev`.

### Phase 4 — `downloader-admin` Rust backend

14. `bins/downloader-admin/{Cargo.toml, AGENTS.md, src/main.rs, src/config/mod.rs,
    src/cmd/{_config,mod,run/{mod,config}}.rs}` — bin skeleton + boot.
15. `src/cmd/run/components/{mod,http_api/{mod,routes/*,auth,envelope,spa},central/{mod,http,rpc}}.rs`.
16. `just dev-build downloader-admin`, `just fmt-dev`. Smoke: log in, retry a
    forced-failed task.

### Phase 5 — Frontend SPA

17. `bins/downloader-admin/frontend/{package.json, vite.config.ts, tsconfig,
    tailwind, postcss, components.json, index.html}`.
18. `src/{main.tsx, routes/, lib/, stores/, components/ui/, components/}`.
19. `cd frontend && bun install && bun run build` → populate `dist/` for
    embedding. `bun run check` clean.

### Phase 6 — Infra + docs

20. `mprocs.yaml` — `admin` proc + optional `admin-fe` proc.
21. `justfile` — add `admin` to `docker-push-all` bin list; document the manual
    `bun run build` step (no new build recipe needed since `dev-watch`/`build`/
    `run` are generic).
22. `docker-bake.hcl` + `.docker/Dockerfile` — new `admin` target/stage
    (bun-build frontend then musl Rust build, like the others).
23. AGENTS.md — new `bins/downloader-admin/AGENTS.md`; root "Binaries" list +
    auth-model note.

---

## File Manifest

| Area | Action |
| --- | --- |
| `crates/app-database/convex/schema.ts` | extend `authed.for` + `connections.role` unions |
| `crates/app-database/convex/requests.ts` | add `getByStatus`, `retry`, `cancel`, `delete` |
| `crates/app-database/convex/authed.ts` | add `listFull`, `create`, `rotateToken`, `revoke`, `remove` |
| `crates/app-database/src/entity/authed/mod.rs` | add `AuthedForRole::Admin` |
| `crates/app-database/src/api/requests/mod.rs` | add 4 typed wrappers |
| `crates/app-database/src/api/authed/mod.rs` | **new** — 5 typed wrappers |
| `crates/app-peer-comms/src/ticket/targeted.rs` | add `TicketTarget::Admin` |
| `crates/app-peer-comms/src/rpc/{request,session,mod}.rs` | `Capabilities::Admin`, `Role::Admin`, 2 new variants + wire types |
| `bins/downloader-central/.../routes/v1/root.rs` | join-ticket admin arm |
| `bins/downloader-central/.../components/rpc/mod.rs` | accept + dispatch admin arms |
| `bins/downloader-admin/**` | **new** — bin + embedded SPA |
| `mprocs.yaml` | add `admin` (+ optional `admin-fe`) |
| `justfile` | add `admin` to `docker-push-all` |
| `docker-bake.hcl` + `.docker/Dockerfile` | new `admin` target/stage |
| `AGENTS.md` (root) + `bins/downloader-admin/AGENTS.md` | docs |

---

## Risks & Trade-offs

### Risk: positional-postcard wire breakage (D2)
New `CentralProtocol` variants must not use `skip`/`skip_serializing_if`; field
counts must match both sides or the peer decodes `Hit the end of buffer`.
Mitigation: round-trip smoke test (admin ↔ central) before merge.

### Risk: `retry` from `done`/`failed` bypasses ownership checks (D5)
The `inProgress` `by === takerId` checks don't apply. Acceptable: the row isn't
currently owned, and admin is privileged. The `CleanupId` (if any) is cancelled.

### Risk: cookie revocation lag (D3)
A stolen cookie is valid up to 12h (sliding) after the token is revoked in
Convex. Mitigation: rotate `--admin-session-secret` on offboarding (invalidates
all cookies); keep exp short-ish for an internal tool.

### Risk: admin bin holds a privileged token (D4)
Same exposure as worker/bot server-side tokens today. Mitigation: gitignored
`.env`, rotate via `authed:rotateToken`.

### Risk: `bun run build` is manual (D8)
`just build downloader-admin` won't reflect FE changes without a manual build.
Mitigation: documented in `bins/downloader-admin/AGENTS.md`; Docker stage does
it for releases.

### Trade-off: no per-user audit in Convex `by` fields (D9)
Convex `by` records the bin's server-side token, not the end-user. Audit via the
bin's request logs (cookie → admin_id).

---

## Open Implementation Details (resolved)

1. **Retry scope** → `failed` **and** `done` (D5).
2. **Browser session** → signed http-only cookie, HMAC-SHA256, 12h sliding (D3).
3. **Bin→central identity** → dedicated server-side admin token from config (D4).
4. **SPA embedding** → `rust-embed` + manual `bun run build`, no `build.rs` (D8).
5. **Central link** → both HTTPS and irpc Admin role (D2).
6. **Auth model** → reuse Convex `authed` with new `admin` role (D3/D6).

---

## Verification Steps

Per root `AGENTS.md`, no test suite. Manual end-to-end via `mprocs`:

1. `just fmt-dev` passes (pedantic+nursery, `unwrap_used` warn).
2. `just dev-build downloader-admin` (and `-central`, `app-database`,
   `app-peer-comms`) compile.
3. Convex: `just db-dev` then `bun run check` clean.
4. Frontend: `cd bins/downloader-admin/frontend && bun run check && bun run build`.
5. Boot mesh via `mprocs` (`db`, `central`, `worker`, `bot-*`, `admin`).
6. **Login**: invalid token → 403; non-admin token → 403; admin token → cookie
   set, dashboard loads.
7. **Requests**: create a task via bot, force-fail it (or wait), confirm it
   appears under the Failed tab; click Retry → status returns to `pending`,
   worker re-takes it; repeat from `done`.
8. **Cancel/Delete**: cancel a pending task (→ failed/cancelled); delete one
   (gone from list).
9. **Tokens**: create a worker token, rotate it, revoke it; confirm old token
   stops working for a fresh worker.
10. **Nodes**: `/connections` lists worker/bot rows with advancing `lastSeen`;
    `/central/sessions` + `/central/parked-workers` return data from irpc.
11. **Wire smoke**: admin ↔ central `Auth` as Admin; both new RPCs round-trip.
12. **Metrics**: `/metrics` renders Prometheus text.
