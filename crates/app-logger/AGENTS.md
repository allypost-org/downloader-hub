# app-logger

`tracing` + `tracing-subscriber` initialiser. Every binary except `downloader-cli` calls `app_logger::init_with_options()` at startup. See root `AGENTS.md` for toolchain/build commands.

## Behaviour

- Installs separate `fmt` layers on stderr and an optional log file, each with its own reloadable `EnvFilter` (levels can change at runtime and be configured independently).
- Stderr defaults to `pretty` (ANSI when stderr is a TTY); file defaults to `plain` (no ANSI). Both are configurable via `--log-format` / `--log-file-format` (or `DOWNLOADER_HUB_LOG_FORMAT` / `DOWNLOADER_HUB_LOG_FILE_FORMAT`). Supported values: `pretty`, `plain`, `json`.
- The file layer uses separate field formatter types (`FileFields` for plain/pretty, `JsonFileFields` for json) so span field caches are not shared with the console layer.
- `COMPONENT_LEVELS` defines default `INFO` levels per known crate/binary; `DOWNLOADER_HUB_LOG_LEVEL` env overrides console logging. File logging can use `DOWNLOADER_HUB_LOG_FILE_LEVEL` and falls back to `DOWNLOADER_HUB_LOG_LEVEL` if not set. Use `--log-file-level` to set a simple base file level from the CLI.
- `COMPONENT_LEVELS` still lists removed crates (`downloader_hub`, `downloader_telegram_bot`, `app_fixers`, etc.) — harmless but stale. Update only if you add a new crate that needs a non-default level.
