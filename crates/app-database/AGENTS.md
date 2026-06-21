# app-database

Convex client for Rust + the Convex TypeScript backend itself. All four binaries share this crate. See root `AGENTS.md` for the dev server command and the "Database" section.

## Two source trees (keep in sync)

- `convex/` — TypeScript. Defines schema + functions. **Source of truth.** Never edit `convex/_generated/` — produced by `convex dev`.
- `src/` — Rust client that mirrors the schema and wraps the websocket Convex client.

## Rust layout

- `src/client.rs` — `Database` singleton (`Database::init(cfg)` → `Database::global()`).
- `src/api/` — request builders per Convex function: `authed/`, `request.rs`, `requests/`.
- `src/entity/` — typed response shapes mirroring `convex/schema.ts`: `authed/`, `requests/`, `_common.rs`.
- `src/helpers/` — serde helpers + the `arg_map!` macro for query args.
- `src/error.rs` — `DatabaseError` / `ResponseError`.

## Schema (`convex/schema.ts`)

Three tables:

- `downloader_hub_authed` — worker/bot auth tokens (`for: "worker" | "bot"`).
- `downloader_hub_requests` — download jobs with a discriminated `status` union: `pending` / `inProgress` / `done` / `failed`.
- `downloader_hub_outbox` — messages broadcast to peer audiences.

When adding a field/table, update **both** `convex/schema.ts` (plus the relevant function file) and the Rust entity mirror under `src/entity/`.
