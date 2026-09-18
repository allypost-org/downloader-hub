# app-macros

Proc-macro crate. Provides the derive used by the `app-config` pattern (see root `AGENTS.md`).

- `Dumpable` - generates serialization for `--dump-config`.

This crate is on the `app-config` critical path - changes here affect every binary's boot sequence.
