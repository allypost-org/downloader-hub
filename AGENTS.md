# AGENTS.md

Repo-specific guidance for OpenCode sessions. Only non-obvious facts are recorded.

## Toolchain

- Rust **stable**, edition **2024**. Workspace = `bins/*` (6 binaries) + `crates/*` (11 libs).
- Task runner is **`just`** (see `justfile`). Prefer `just <recipe>` over raw `cargo` for dev/build/migrate.
- Required external tools (verify with `which`): `just`, `cargo`, `sea-orm-cli`, `watchexec`, `bun`, `mprocs`.
- Runtime binaries the app shells out to: `yt-dlp`, `ffmpeg`, `ffprobe`, optionally `scenedetect`.

## Commands

- `just dev` → run `downloader-hub` with watchexec (default recipe). Equivalent: `just dev-watch-server`.
- Per-binary dev: `just dev-watch <package> <args...>`.
- Run once without watching: `just dev-run <package> [args]`.
- Build a binary (MUSL static, `+crt-static`): `just build <bin>`.
- Lint: `just lint` / `just lint-fix`. Format: `just fmt`. `just fmt-dev` uses nightly rustfmt.
- All services at once: `mprocs` (uses `mprocs.yaml`).

There is no test suite. Do not invent `cargo test` invocations; the repo has one annotated test file only.

## Lint & style rules (enforced)

- Clippy **pedantic + nursery** are on at `warn`. `unwrap_used = "warn"` — avoid `.unwrap()` in new code; use `?` / `expect` only with reason.
- rustfmt: `group_imports = "StdExternalCrate"`, `imports_granularity = "Crate"`, `imports_layout = "Vertical"`, `format_strings = true`. 4-space indent for `.rs`.
- Each crate owns its own `Config` (`pub(crate)`); initialize with `Config::init_parsed()` then read `Config::global()`. See `bins/downloader-hub/src/main.rs:20` for the canonical boot sequence.

## Two databases (important)

This branch (`feat/bot-database`) is mid-migration. Both stacks are live:

1. **PostgreSQL via SeaORM** — used only by `downloader-hub`.
   - Entities are **generated**, not hand-written: `crates/app-entities/src/entities/`. Do not edit; regenerate with `just generate-entities` (runs `sea-orm-cli generate entity`).
   - Migrations live in `crates/app-migration/src`. Run via `just migrate up` (or `just migrate <ARGS...>`). **`just migrate` always regenerates entities afterward** — keep generated files in sync when you change schema.
   - Create a migration: `just migration-create <name>`.

2. **Convex** (TypeScript backend, hosted DB) — used by `downloader-central`, `downloader-worker`, `downloader-bot`.
   - Schema/functions: `crates/app-database/convex/`. **Never edit `crates/app-database/convex/_generated/`** — produced by `convex dev`.
   - Rust client crate: `app-database` (in `crates/app-database/src/`).
   - Dev server: `just db-dev` (runs `bun run dev` inside `crates/app-database`), or use the `db` entry in `mprocs.yaml`. Needs `crates/app-database/.env.local`.

When adding a field/table: update **both** the relevant Convex schema (`crates/app-database/convex/schema.ts`) and the SeaORM migration+entities if the hub binary must also see it.

## Process architecture

`downloader-hub` is the legacy standalone server (PostgreSQL). The newer peer-comms services (`central`, `worker`, `bot`) communicate via **iroh** (`app-peer-comms` crate) and share state through Convex. `bot` is multi-platform (Telegram + Discord) selected via subcommand; see `mprocs.yaml` for the dev invocations and dev-only `--peer-comms-*` flags.

## Env / secrets

- `.env` at repo root is the source of dev config but is **gitignored**; copy values from a teammate if missing. `just` enables `dotenv-load`, so `just <recipe>` picks it up automatically.
- `.env` contains real-looking dev tokens (Telegram, Discord). Do not echo secrets into commits, logs, or PRs.
- `DATABASE_URL` (PG) and `DOWNLOADER_HUB_DATABASE_URL` (Convex URL, port 3210) are distinct — do not confuse them.

## CI / deploy

`.github/workflows/deploy.yaml` triggers on push to `main`. It builds four Docker images (`downloader-worker`, `downloader-central`, `downloader-bot`, `downloader-telegram-bot`) and pushes to Docker Hub; a Watchtower webhook then rolls deployments. The `downloader-hub` image build is commented out — don't assume CI covers it.

## Misc gotchas

- `.gitattributes` forces `linguist-language=TypeScript` repo-wide — GitHub language stats are misleading; the codebase is predominantly Rust.
- `watchexec` is used for dev (not `cargo-watch`). `cargo watch` is only used by `dev-watch-build*`.
- Release profile: `lto = "thin"`, `codegen-units = 1`, `strip = true`. There is a separate `release-cli` profile (`opt-level = "s"`, `panic = "abort"`) used by `just install-cli`.
