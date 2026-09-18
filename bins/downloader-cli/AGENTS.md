# downloader-cli

Local CLI tool. Directly invokes `app_actions` to download/fix files - **not** part of the peer-comms mesh. See root `AGENTS.md` for toolchain/build commands.

## Behaviour

- Single-file binary: `src/main.rs` (`#[tokio::main]`).
- Uses its own minimal `tracing_subscriber` setup, **not** `app_logger`.
- Reads URLs and/or file paths from CLI args / config.
- Builds `Actions` in `main` from the parsed `Config` (`ActionCtx` from the endpoint/dependency-paths/request configs + the disabled-entries set); the `--no-auto-crop` flag extends the disabled set with the crop fixers in its `BootConfig::init_parsed` override.
- Calls `actions.download_file(...)` then `actions.fix_file(...)` on the `Actions` registry.
- Some `Action` handlers (e.g. `RenameToId`, `SplitScenes`) are applied after download.

## Install

- `just install-cli` (uses the `release-cli` profile).
- Optional `INSTALL_LOCATION` env var moves the binary post-install.
