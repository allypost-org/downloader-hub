# AGENTS.md

## Per-package docs

Each binary (`bins/*`) and crate (`crates/*`) has its own `AGENTS.md` with layout, entrypoints, and startup contracts. Read the relevant one before editing a package — facts recorded there are intentionally not duplicated here.

- **Binaries:** `downloader-central`, `downloader-worker`, `downloader-bot`, `downloader-cli`, `downloader-admin`.
- **Crates:** `app-actions` (download/process pipeline), `app-config` (config infra), `app-database` (Convex client + backend), `app-peer-comms` (iroh wrapper), `app-helpers`, `app-tasks`, `app-logger`, `app-macros`, `app-requests`. (`app-entities` is legacy/orphaned — see its `AGENTS.md`.)

## Toolchain

- Rust **stable**, edition **2024**. Workspace = `bins/*` (5 binaries) + `crates/*` (9 libs).
- Task runner is **`just`** (see `justfile`). Prefer `just <recipe>` over raw `cargo` for dev/build.
- Required external tools (verify with `which`): `just`, `cargo`, `watchexec`, `bun`.
- Runtime binaries the app shells out to: `yt-dlp`, `ffmpeg`, `ffprobe`, optionally `scenedetect`.
- Build target and rustflags are pinned globally in `.cargo/config.toml`: `target = "x86_64-unknown-linux-musl"`, `-C target-feature=+crt-static`, and `--cfg tokio_unstable`. Plain `cargo build`/`cargo run` pick these up automatically — do not re-pass `--target` or `RUSTFLAGS`.

## Commands

- `just dev-watch <package> [args...]` → run a binary with `watchexec` (auto-restart on change).
- `just dev-run <package> [args...]` → run once without watching.
- `just dev-build <package> [args...]` → dev build of a single package.
- `just build <bin>` → release build of a binary (the canonical production build).
- `just run <bin> [args...]` → release build + run.
- `just db-dev` → Convex dev server (`bun run dev` inside `crates/app-database`).
- `just fmt-dev` → use for all lint/fix operations. Don't use `cargo fmt` or `cargo clippy` directly. Don't use `just fmt` or `just lint` directly unless you have a very good reason.
- Install `downloader-cli`: `just install-cli` (uses the `release-cli` profile).

There is no test suite. Do not invent `cargo test` invocations; the repo has one annotated test file only.

## Lint & style rules (enforced)

- Clippy **pedantic + nursery** are on at `warn`. `unwrap_used = "warn"` — avoid `.unwrap()` in new code; use `?` / `expect` only with reason.
- rustfmt: `group_imports = "StdExternalCrate"`, `imports_granularity = "Crate"`, `imports_layout = "Vertical"`, `format_strings = true`. 4-space indent for `.rs`.
- Each crate owns its own `Config` (`pub(crate)`); initialize with `Config::init_parsed()` then read `Config::global()`. See `bins/downloader-central/src/main.rs` for the canonical boot sequence.

## Database

State lives in **Convex** (hosted TS backend). All five binaries share it through the `app-database` Rust client crate.

- Schema/functions: `crates/app-database/convex/`. **Never edit `crates/app-database/convex/_generated/`** — produced by `convex dev`.
- Dev server: `just db-dev` or the `db` entry in `mprocs.yaml`. Needs `crates/app-database/.env.local`.
- Add a field/table: edit `crates/app-database/convex/schema.ts` (and the relevant Convex function files); the Rust client in `crates/app-database/src/` mirrors the schema.

## Process architecture

Five binaries:

- `downloader-central` — iroh node + coordination point. Serves the irpc control protocol (`app_peer_comms::rpc::CentralProtocol`) over QUIC, publishes pub/sub broadcasts on iroh-gossip, and runs a **minimal** axum HTTP surface (`/api/v1/join-ticket` bootstrap, `/api/v1/metrics`, `/health`).
- `downloader-worker` — performs downloads/processing.
- `downloader-bot` — multi-platform (Telegram + Discord + Others) selected via subcommand.
- `downloader-cli` — local CLI tool.
- `downloader-admin` — operator UI (embedded React 19 SPA). Monitors system state and performs privileged actions (retry/cancel/delete requests, manage authed tokens). Reads Convex directly + scrapes central over HTTP + irpc `Admin` role. Browser sessions are signed-cookie-authenticated against Convex `authed` rows with `for = "admin"`. See `docs/plans/2026-07-03_18-44_admin-bin-embedded-react-app.md`.

`central`, `worker`, and `bot` (and `admin`) communicate over **iroh QUIC**: request/response RPC via **`irpc`** (riding the existing `PeeringEndpoint` through the `irpc-iroh` transport; ALPN `app_peer_comms::rpc::RPC_ALPN`), and fan-out broadcasts (available work, frees, failures) via **iroh-gossip**. File transfer uses iroh-blobs. Auth is **API-key-at-connect** (the `Auth` irpc call; validated against Convex `downloader_hub_authed`) — there are no JWTs on the RPC path. `authed.for` is `worker` | `bot` | `admin`; `admin`-role irpc calls are read-only (`AdminListSessions`, `AdminListParkedWorkers`) and gate on `is_admin` in central's dispatch. See `docs/plans/2026-07-01_14-54_move-control-rpc-to-irpc.md` for the full design and `mprocs.yaml` for dev invocations / dev-only `--peer-comms-*` flags.

## Env / secrets

- `.env` at repo root is the source of dev config but is **gitignored**; copy values from a teammate if missing. `just` enables `dotenv-load`, so `just <recipe>` picks it up automatically.
- `.env` contains real-looking dev tokens (Telegram, Discord) and some stale keys from removed binaries — do not echo secrets into commits, logs, or PRs.

## CI / deploy

`.github/workflows/deploy.yaml` triggers on push to `main`. A single `docker buildx bake` invocation builds all three images — `downloader-worker`, `downloader-central`, `downloader-bot` — from the unified `.docker/Dockerfile` (see `docker-bake.hcl`); the shared `chef`/`planner`/`deps` stages compile the workspace dependency graph once across all three. Images are pushed to Docker Hub; a Watchtower webhook then rolls deployments.

## Misc gotchas

- `watchexec` is the only watcher used; all `dev-watch*` recipes route through the `_watch` helper.
- Release profile: `lto = "thin"`, `codegen-units = 1`, `strip = true`. The separate `release-cli` profile (`opt-level = "s"`, `panic = "abort"`) is used only by `just install-cli`.
- DO NOT write ANY unnecessary code comments. Only write them if they are **_absolutely necessary_** to understand the code (in which case it's probably better to re-write it to be more readable) OR if they're doc comments for clap attributes (help text is auto-generated from them).
