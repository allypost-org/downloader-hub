# Config Dependency Injection

Date: 2026-09-02
Status: design doc - direction approved; implementation plan to follow

## Goal

Replace the process-global configuration architecture with explicit dependency
injection of plain config values and runtime handles. Today "get config" goes
through nine different process-wide singletons (per-crate `GlobalConfig`
OnceLocks, a hand-rolled one in `app-database`, per-bin `RpcClient` globals,
`TelegramBot::instance()` / `DiscordBot::instance()`, central's `rpc::init_*`
statics), all with implicit init-order contracts enforced by runtime panics and
deliberately swallowed init errors. After this refactor, dependencies are
visible in function signatures, initialization order is enforced by the
compiler, config-dependent code is testable with plain values, and the only
remaining globals are the tracing reload handles in `app-logger`, which are
inherent to how `tracing` works.

The blast radius is large (~100 `::global()` call sites) but mechanical, and
the migration is staged so every step compiles on its own.

## User Decisions And Constraints

- Approved direction: dependency injection (values/`Arc`s passed down; delete
  the `GlobalConfig` derive and trait; keep globals only for `tracing`).
- Disabling actions is per-run config consulted at enumeration time. The
  registry keeps all entries plus the disabled set as data; there is **no**
  construction-time filtering. The `list` subcommand must keep showing entries
  regardless of disabled state, and future dynamic disables stay possible.
- Rename `DisableEntry` to `EntryId`, add an `AsEntryId` trait, and type the
  category so no bare category/name strings float through the code.
- CLI flags and env var names are a deployed interface and do not change
  (`--disable-action-entry`, `DOWNLOADER_HUB_DISABLED_ACTION_ENTRIES`, all
  `DOWNLOADER_HUB_*` names). Values keep their current meaning and parsing.
- The dynamic-settings channels (log levels and secrets via Convex watch +
  irpc heartbeat, per-request max filesize inside `FileReference`) are
  unchanged by this refactor. The static config is immutable after parse
  today and stays immutable; runtime mutation keeps flowing through the
  existing reload handles, watchers, and per-request fields.
- The value-level `Dumpable` machinery (config dump, shell completions) stays.
- Verification is `just fmt-dev` plus per-stage builds; the repo has no test
  suite and this refactor does not add one. Testability is an outcome the new
  architecture enables, not a deliverable of this refactor.

## Current Architecture

### Machinery

- `#[derive(GlobalConfig)]` (`crates/app-macros/src/lib.rs`) plants a
  `static OnceLock<Self>` per config struct and generates `Config::global()`,
  which panics with "Config not initialized" when read before `init()`.
- Each of the five bins carries a near-identical `Config` (clap `Parser` +
  serde + `Validate` + `GlobalConfig` + `Dumpable`) with a copy-pasted
  `init_parsed()` template: parse -> `resolve_paths` -> `validate_or_exit` ->
  `dump_if_needed` -> `init`.
- Three crates own `pub(crate)` configs through the same derive:
  `app-actions::ActionsConfig`, `app-helpers::HelpersConfig`,
  `app-tasks::TaskConfig`. `app_helpers::config::init` is also called from
  inside `app_actions::config::init` with its error swallowed, creating two
  sources of truth for `dependency_paths`.

### Pain points (evidence)

1. **Init-order coupling is invisible until it panics.** dotenv before clap;
   bin config before logger init (central's `AGENTS.md` documents the wrong
   order); `app_actions::config::init` before any `is_enabled()` call;
   `PeeringEndpoint::init` before every `RpcClient::init` (all three read
   `PeeringEndpoint::global().router.endpoint()` during init); central's five
   `rpc::init_*` calls before `init_peering` because the router hook serves
   immediately; `Database::init` before any `Database::global()`.
2. **Init errors are deliberately swallowed.** The worker does
   `_ = app_tasks::config::init(...)`, `_ = app_helpers::config::init(...)`,
   `_ = app_actions::config::init(...)` so `keep_running` restarts don't fail
   on "already initialized". A restarted component constructed with different
   flags silently keeps the old process-lifetime config; genuine init failures
   vanish the same way. Admin's `PeeringEndpoint::init` warn-and-reuses on the
   same grounds.
3. **Dependencies are hidden.** Deep handlers read globals ad hoc: `ffprobe`
   resolves its binary from the global on every call
   (`crates/app-helpers/src/ffprobe.rs`), extractors read User-Agent strings
   per request, `is_enabled()` consults `ActionsConfig::global()` on every
   registry enumeration, and admin/central call `Database::global()` ~84
   times combined. Nothing touching config can be exercised without mutating
   process-global state.
4. **Four coexisting idioms for "global singleton"**: the `GlobalConfig`
   derive, `app-database`'s hand-rolled OnceLock, `PeeringEndpoint` deriving
   `GlobalConfig` despite being a live mutable runtime object, and the bins'
   `instance()`/`init()` statics.
5. **Five copies of the bin bootstrap template** with tiny env-var
   differences.

What is already fine and stays: shared config types in
`app_config::common`/`conditional`, the clap + env + dotenv merge, and the
dynamic-settings distribution described under constraints.

## Design

### Layering (target)

1. `app-config` keeps its role as the home of shared config *types* plus
   validators, and gains the typed entry-id types and a shared bootstrap
   helper. It no longer hosts the `GlobalConfig` trait (deleted).
2. Library crates consume config **by value at construction** and store what
   they need (`Arc`-wrapped) in their runtime objects. They no longer own
   config globals.
3. Bins parse their `Config` once in `main` (as a plain value, not a global),
   build a small runtime struct, and pass it down.

### Typed entry ids

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryCategory { Action, Downloader, Extractor, Fixer }

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntryId {
    pub category: EntryCategory,
    pub name: String, // lowercased, as today
}

pub trait AsEntryId {
    fn entry_id(&self) -> EntryId;
}
```

- `DisabledEntriesConfig` keeps its clap flag/env verbatim but its field
  becomes `Vec<EntryId>`. `EntryId::from_str` still parses `category:name`
  and now rejects unknown categories with a helpful error instead of
  accepting any string.
- The four pipeline traits (`Action`, `Downloader`, `Extractor`, `Fixer`) each
  gain a default `entry_id()` method that pairs their fixed category with
  their existing `name()`. `AsEntryId` provides the single generic bound used
  by `Actions::is_enabled`. Exact coherence mechanics (impls for the `dyn`
  types vs a per-trait method used directly) are settled in the
  implementation plan; the behavioral contract is: no call site constructs an
  `EntryId` from string literals anymore.
- The trait-default `is_enabled()` methods on the four traits and the
  `From<(C, N)>` tuple impl are deleted. The existing call sites
  `--no-auto-crop` included (which extends the disabled set after parse and
  before registry construction, exactly as today) keep working unchanged.

### `app-actions`: the `Actions` registry

```rust
pub struct ActionCtx {
    pub endpoint: Arc<EndpointConfig>,
    pub dependency_paths: Arc<ProgramPathConfig>,
    pub request: Arc<RequestConfig>,
}

pub struct Actions {
    ctx: Arc<ActionCtx>,
    disabled: HashSet<EntryId>,
}

impl Actions {
    pub fn new(ctx: Arc<ActionCtx>, disabled: impl IntoIterator<Item = EntryId>) -> Self;

    pub fn is_enabled(&self, id: &impl AsEntryId) -> bool;

    pub fn enabled_actions(&self) -> impl Iterator<Item = &ActionEntry>;
    pub fn enabled_downloaders(&self) -> impl Iterator<Item = &DownloaderEntry>;
    pub fn enabled_fixers(&self) -> impl Iterator<Item = &FixerInstance>;
    // extractors analogous
}
```

- `ActionsConfig`, its `endpoints()`/`request()`/`dependency_paths()`
  accessors, and `app_actions::config::init` (including its hidden
  `app_helpers` re-init) are deleted. `Actions` is the single object callers
  hold; the pipeline entry points (`download_file`, `fix_file`, `extract_info`,
  post-actions) become methods on it or free functions taking `&Actions`.
- Disables are consulted at enumeration time (per-run config, not
  construction-time filtering), preserving current semantics, the `list`
  subcommand's show-everything behavior, and the possibility of dynamic
  disables later.
- The `ExtractInfoRequest::extractors()`/`available_extractors()` methods move
  to `Actions` (a request should not know about the registry); the request
  remains plain data.
- Handler trait methods that read config gain an `&ActionCtx` parameter
  (primary execution methods such as `download`/`extract_info`/fix runs;
  predicates like `can_download`/`can_handle` don't). Handlers read
  `ctx.request`, `ctx.endpoint`, `ctx.dependency_paths` instead of the global
  accessors. The per-handler churn is the price of visible dependencies and is
  accepted.

### `app-helpers`

- `HelpersConfig` and `app_helpers::config::init` are deleted.
- `ffprobe*` and the other path-consuming helpers take the relevant config by
  parameter (e.g. `paths: &ProgramPathConfig` or the resolved binary path).
  Callers in `app-actions` pass `&ctx.dependency_paths`.
- `cache_dir()` is already global-free and stays.

### `app-tasks`

- `TaskConfig` (the wrapper + `Deref`) and its global are deleted.
  `TaskRunner::run` takes the `app_config::common::TaskConfig` by value (the
  interval is captured into the spawned loop once, as today).

### `app-peer-comms`

- `PeeringEndpoint` stops deriving `GlobalConfig`. It is constructed by its
  builder and handed around as `Arc<PeeringEndpoint>`.
- `download_ticket_into` (an associated function today only because it reaches
  for the global) becomes a `&self` method. The internal lazy `DOWNLOADER`
  OnceLock becomes endpoint state.

### `app-database`

- The hand-rolled `GLOBAL` OnceLock, `Database::global()`, and
  `Database::init` are removed in favor of a constructor returning the
  client; callers hold `Arc<Database>` (central's components, central's rpc
  state, admin's axum state, admin's log-settings watcher).
- `DatabaseError::AlreadyInitialized` disappears.

### `app-logger` (unchanged)

- Keeps its `OnceLock` reload handles and `init_with_options` - `tracing`
  subscribers are process-global by nature. Bins already pass log options by
  value; nothing changes here.

### Bins

- A shared helper in `app-config` replaces the five copies of the
  `init_parsed` template (parse -> resolve -> validate-or-exit -> dump-if-needed
  -> return **owned** `Config`). Bins keep their tiny bin-specific
  `resolve_paths`. Bins' `Config` structs no longer derive `GlobalConfig`;
  `main` owns the value and passes it down (the CLI's fold of
  `app_actions::config::init` into `init_parsed` disappears along with it).
- Each long-running bin gets a small runtime struct holding its handles, built
  after parsing, passed into components:
  - **central**: a `CentralState` (or equivalent) carrying `Arc<Database>`,
    `Arc<PeeringEndpoint>`, and the session/distributor/restrictions/log-settings/secrets
    state currently living in the five `rpc::init_*` statics; the router hook
    closure captures it.
  - **worker**: the `keep_running` closure owns its `RpcClient`, constructed
    fresh per restart (replacing init-once-in-a-global); mid-run `reauth`
    survives unchanged as a method on the owned client, with the
    `ArcSwapOption` moving inside it; the tag-cleanup loop and
    `fix_stage_and_deliver` receive the
    `Arc<PeeringEndpoint>` and `Arc<Actions>` they currently fetch from
    globals.
  - **bot**: `TelegramBot`/`DiscordBot` statics become values stored in the
    platform framework's state container (serenity's `TypeMap` / the
    telegram client's data) so handlers get them through the framework
    instead of `instance()`.
  - **admin**: axum router state carries `Arc<Database>` and the central
    client slot it already holds; the `log_settings` watcher takes the
    database by value.
  - **cli**: constructs `Actions` directly in `main` from its parsed config
    (including the `--no-auto-crop` disabled-set extension) and passes it to
    the pipeline calls.
- Worker/bot `RpcClient` singletons and their `init`/`global` pairs are
  deleted; an `RpcClient` is constructed where its inputs
  (`Arc<PeeringEndpoint>`, key, capabilities) are available.

### Error handling

- Nothing swallows initialization anymore: constructing `Actions`, the
  database client, or the peering endpoint returns real `Result`s that
  propagate. `keep_running` restart loops re-construct their state from the
  still-owned config values, so a restarted component always matches the
  config it was launched with (today's init-once semantics silently reuse the
  first config).

## Migration Plan

Each stage compiles and can land independently.

1. **Typed ids in `app-config`**: rename `DisableEntry` -> `EntryId`, add
   `EntryCategory` + `AsEntryId`, port the four traits and call sites. CLI/env
   surface unchanged.
2. **Bootstrap dedup**: shared parse/validate/dump helper in `app-config`;
   five bins adopt it (still returning owned configs; `GlobalConfig` derive
   can stay temporarily).
3. **Small crates**: `app-helpers` and `app-tasks` switch to parameters;
   delete their globals and the hidden `app_helpers` re-init.
4. **`app-actions`**: introduce `ActionCtx` + `Actions`, thread `&ActionCtx`
   through handler methods, move enumeration to `Actions`, delete
   `ActionsConfig`.
5. **`app-peer-comms` + `app-database`**: drop their globals; thread
   `Arc<PeeringEndpoint>` / `Arc<Database>` through central and admin (the
   bulk of the mechanical call-site updates).
6. **Bins finish**: runtime structs replace `RpcClient`/bot statics and
   central's `rpc::init_*`; bins' `Config` structs drop `GlobalConfig`;
   delete the `GlobalConfig` proc-macro, trait, and remaining
   `OnceLock`-singleton idioms; update the per-bin `AGENTS.md` startup
   contracts (including central's incorrect logger/config ordering note).

## Out Of Scope

- Adding a test suite (enabled by, not part of, this refactor).
- Any change to the dynamic-settings channels, Convex schema, or wire
  protocols.
- Renaming CLI flags or env vars.
- The `Dumpable`/dump machinery beyond what the bootstrap dedup touches.
- `app-entities` (legacy/orphaned).
