# app-macros

Proc-macro crate. Provides two derives used by the `app-config` pattern (see root `AGENTS.md`).

- `GlobalConfig` — generates a per-type `OnceLock`-backed singleton + `global()`, `init()`, `init_global()`, `global_initialized()` accessors. Backs the `Config::global()` convention every binary relies on.
- `Dumpable` — generates serialization for `--dump-config`.

This crate is on the `app-config` critical path — changes here affect every binary's boot sequence.
