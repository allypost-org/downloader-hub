# app-actions

Core download/process pipeline. Used by `downloader-cli` and `downloader-worker`. See root `AGENTS.md` for toolchain/build commands.

Pipeline: `extractors` → `downloaders` → `fixers` → `actions`.

## Adding a handler

Each handler category has a `src/<category>/handlers/mod.rs` with `LazyLock<Vec<Arc<dyn Trait>>>` registries: `ALL_*` and `AVAILABLE_*`.

1. Implement the trait (`Extractor`, `Downloader`, `Fixer`, or `Action`).
2. Add it to the `all_*()` constructor in the matching `handlers/mod.rs`.
3. `AVAILABLE_*` is derived from `ALL_*` filtered by `is_enabled()` / `can_run()` (some probe for external binaries at startup).

Production code iterates `AVAILABLE_*` — registering in `all_*()` is enough; don't touch `AVAILABLE_*` directly.

## Layout

- `src/extractors/handlers/` — per-site URL → `ExtractedInfo` (twitter, reddit, youtube, instagram, tiktok, tumblr, threads, bsky, activity_pub, imgur, music, fallthrough).
- `src/downloaders/handlers/` — `yt_dlp`, `generic`, `music`.
- `src/fixers/handlers/` — post-processing (file_name, file_extensions, media_formats, crop_image, crop_video_bars).
- `src/actions/handlers/` — discrete user-triggerable actions (split_scenes, ocr_image, remove_background, compact_media, file_rename_to_id).
- `src/common/` — shared types (`DownloadRequest`, `DownloadResult`).
- `src/config.rs` — `init(endpoint, dependency_paths, disabled_entries, request)` must be called before any handler runs.

Top-level `download_file()` and `fix_file()` are the public entrypoints.
