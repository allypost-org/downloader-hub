# app-helpers

Grab-bag of utility modules. No single theme; mostly thin wrappers around std / tokio / external binaries. See root `AGENTS.md` for toolchain/build commands.

## Notable modules

- `config` — `app_helpers::config::init(dependency_paths)` wires runtime binary paths (yt-dlp / ffmpeg / ffprobe / scenedetect). Called by `downloader-bot` and `downloader-worker` at startup.
- `futures` — `run_future`, `TaskController`, `retry_future` (`keep_running`, `run_retried`, `RetryConfig`). Used by every long-running binary.
- `ffprobe` — shells out to `ffprobe`.
- `file_*` — file type / size / time / name helpers.
- `trash` — moves files to OS trash via the `trash` crate.
- `temp_dir`, `temp_file` — RAII temp artifacts.
- `tree_yielder` — async tree walker. The only file in the repo with a `#[test]`.
- `ip`, `domain` — network/DNS helpers.
- `unique_vec`, `results`, `encoding`, `id`, `dirs` — small utilities.
