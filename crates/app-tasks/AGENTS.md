# app-tasks

Cron-style task runner. `downloader-worker` spawns `TaskRunner::run()` alongside its main loop. See root `AGENTS.md` for toolchain/build commands.

## Layout

- `src/lib.rs` — `TaskRunner::run()` spawns a blocking thread that runs the cron schedule.
- `src/config.rs` — `init(TaskConfig)` sets the global cron config; must be called before `run()`.
- `src/cron/` — the actual scheduled jobs.
