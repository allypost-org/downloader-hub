# app-database

Convex client for Rust + the Convex TypeScript backend itself. All five binaries share this crate. See root `AGENTS.md` for the dev server command and the "Database" section.

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

Four tables:

- `downloader_hub_authed` — auth tokens (`for: "worker" | "bot" | "admin"`).
- `downloader_hub_requests` — download jobs with a discriminated `status` union: `pending` / `inProgress` / `delivering` / `done` / `failed`. `delivering` is the bot delivery lease (fenced by `deliveryAttemptId`); `done.by` is the worker, optional `done.deliveredBy` is the delivering bot.
- `downloader_hub_outbox` — messages broadcast to peer audiences.
- `downloader_hub_connections` — peer inventory rows (one per authed peer per central).

When adding a field/table, update **both** `convex/schema.ts` (plus the relevant function file) and the Rust entity mirror under `src/entity/`.

## Components & aggregates

`convex.config.ts` installs the `@convex-dev/aggregate` component as
`requestCounts`, backing the `requests:counts` query with O(log n) lookups.

- `convex/lib/requestCounts.ts` — `TableAggregate` namespaced by `status.Type`.
- `convex/lib/triggers.ts` — a `Triggers` registration that keeps the aggregate
  in sync on every `ctx.db.insert/patch/delete` against `requests`. **Every
  `requests:*` mutation is wrapped** (`customMutation(raw, customCtx(triggers.wrapDB))`)
  so the trigger fires automatically — do not add a raw `mutation` export in
  `requests.ts`; reuse the wrapped `mutation`/`internalMutation` at the top.
- `requests:backfillCounts` — one-shot internal mutation to populate the
  aggregate from existing rows. Run once after first deploying the component:
  `npx convex run requests:backfillCounts '{}'` (then never again).

When the component is first added/renamed, `convex dev` must be **restarted**
(a config change is not always hot-reloaded) so `_generated/api.ts` regenerates
with the `components.requestCounts` export.

ALWAYS run `bun run check` after making changes to any of the convex files.
