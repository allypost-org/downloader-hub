# app-logger

`tracing` + `tracing-subscriber` initialiser. Every binary except `downloader-cli` calls `app_logger::init()` at startup. See root `AGENTS.md` for toolchain/build commands.

## Behaviour

- Installs a `fmt` layer on stderr with an `EnvFilter` behind a `reload::Handle` (log level can change at runtime).
- `COMPONENT_LEVELS` defines default `INFO` levels per known crate/binary; `DOWNLOADER_HUB_LOG_LEVEL` env overrides.
- `COMPONENT_LEVELS` still lists removed crates (`downloader_hub`, `downloader_telegram_bot`, `app_fixers`, etc.) — harmless but stale. Update only if you add a new crate that needs a non-default level.
