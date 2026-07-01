# downloader-worker

Fetches download jobs from central via peer-comms and processes them using `app-actions`. See root `AGENTS.md` for toolchain/build commands.

## Layout

- `src/cmd/work/app.rs` — main loop, wrapped in `keep_running("Worker", …)` with a bounded retry schedule.
- `src/cmd/work/global.rs` — process-global state.
- `src/cmd/work/config.rs` — clap `WorkerConfig` (peer-comms ticket/api, dependency paths, endpoints, request config).
- `src/cmd/list.rs` — `list` subcommand: dumps available downloaders/fixers/actions/extractors.

## Startup contract (`async_run` in `src/cmd/work/mod.rs`)

1. `app_tasks::config::init(...)` + `app_helpers::config::init(...)` + `app_actions::config::init(...)` — must happen before any processing.
2. `init_peering_endpoint(config.peer)` — either parses a pre-provided ticket from config, or fetches one from the central API at `/api/v1/join-ticket`.
3. Spawns `TaskRunner::run()` plus an expired-tag cleanup loop (10 s interval).
4. Runs the main worker loop with the retry schedule in `async_run`.

The retry schedule resets after `TargetedJwtConfig::default_token_expiration_duration() - 30s` — keep this in sync with JWT expiry if you touch it.
