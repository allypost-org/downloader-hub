# Config Dependency Injection - Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace all process-global config/runtime singletons (`GlobalConfig` derive OnceLocks, `Database::global()`, `PeeringEndpoint::global()`, per-bin `RpcClient`/bot statics) with values passed explicitly (`Arc`), keeping globals only for `tracing` reload handles in `app-logger`.

**Architecture:** Six-stage refactor matching the design doc `docs/plans/2026-09-02_12-28_config-dependency-injection.md`. Config types stay in `app-config`; crates consume config by value at construction; each bin parses once and threads a runtime struct down. Typed `EntryId`/`EntryCategory`/`AsEntryId` replace stringly `DisableEntry`.

**Tech Stack:** Rust stable, edition 2024, workspace of 5 bins + 9 crates. Build via `just` (see Global Constraints).

## Global Constraints

- **No test suite exists. Do NOT run or invent `cargo test`.** Verification per task: `just fmt-dev` (workspace clippy --fix + fmt; must pass with no errors) plus the `just dev-build <package>` commands named in the task.
- Never edit `crates/app-database/convex/_generated/`.
- CLI flags and env var names are frozen: `--disable-action-entry`, `DOWNLOADER_HUB_DISABLED_ACTION_ENTRIES`, all `DOWNLOADER_HUB_*`. Parsing semantics (lowercasing, `category:name` split, `,` delimiter) stay identical except unknown categories now error.
- Clippy pedantic + nursery at `warn`, `unwrap_used = "warn"` - use `?` or `expect` with a reason message. No comments unless absolutely necessary.
- **Commit discipline:** the working tree contains unrelated WIP. Stage only files you changed (list them explicitly in `git add`). Commit messages match repo style: short imperative, no prefixes (e.g. "Move worker config to dependency injection").
- Every task leaves the workspace compiling (`cargo check --workspace` via `just fmt-dev`). Tasks are ordered; do not reorder.
- Anchor edits to surrounding code content (function/struct names), not line numbers - the tree contains WIP that may shift lines.
- `.cargo/config.toml` pins target/rustflags; plain `cargo`/`just` picks them up. Never pass `--target` or `RUSTFLAGS`.

## File Structure (new files)

- `crates/app-config/src/common/entry_id.rs` - `EntryCategory`, `EntryId`, `AsEntryId` (Task 1)
- `crates/app-config/src/traits/boot_config.rs` - `BootConfig` bootstrap trait (Task 2)
- `crates/app-actions/src/ctx.rs` - `ActionCtx` (Task 4)
- `crates/app-actions/src/registry.rs` - `Actions` registry (Task 4)
- `bins/downloader-central/src/cmd/central/components/state.rs` - `CentralState` (Task 5)

Everything else modifies existing files. Deleted across the plan: `crates/app-config/src/traits/global_config.rs`, the `GlobalConfig` proc-macro in `crates/app-macros/src/lib.rs`, `crates/app-actions/src/config/mod.rs`, `crates/app-helpers/src/config/mod.rs`, `crates/app-tasks/src/config/mod.rs`.

---

## Stage 1 - Typed entry ids

### Task 1: `EntryId` / `EntryCategory` / `AsEntryId` and full rename from `DisableEntry`

**Files:**
- Create: `crates/app-config/src/common/entry_id.rs`
- Modify: `crates/app-config/src/common/disabled_entries.rs` (full rewrite below)
- Modify: `crates/app-config/src/common/mod.rs` (add `pub mod entry_id;` + `pub use entry_id::*;`)
- Modify: `crates/app-actions/src/config/mod.rs`
- Modify: `crates/app-actions/src/actions/mod.rs`, `src/downloaders/mod.rs`, `src/extractors/mod.rs`, `src/fixers/mod.rs`
- Modify: `bins/downloader-cli/src/config/mod.rs`

**Interfaces:**
- Produces: `app_config::{EntryCategory, EntryId, AsEntryId}` where `EntryCategory = {Action, Downloader, Extractor, Fixer}` (serde lowercase), `EntryId { category: EntryCategory, name: String }` with `EntryId::new(EntryCategory, impl AsRef<str>)` (lowercases name), `FromStr for EntryId` parsing `"category:name"`, `trait AsEntryId { fn entry_id(&self) -> EntryId; }` with blanket impls for `&T`, `Arc<T>`, `Box<T>`. `DisabledEntriesConfig { entries: Vec<EntryId> }` (flag/env unchanged).
- Produces: `AsEntryId` impls for `dyn Action`, `dyn Downloader`, `dyn Extractor`, `dyn Fixer + Send + Sync`, `dyn Extractor + Sync + Send` (exact recipient types below).

- [ ] **Step 1: Create `crates/app-config/src/common/entry_id.rs`**

```rust
use std::{str::FromStr, sync::Arc};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryCategory {
    Action,
    Downloader,
    Extractor,
    Fixer,
}

impl EntryCategory {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Action => "action",
            Self::Downloader => "downloader",
            Self::Extractor => "extractor",
            Self::Fixer => "fixer",
        }
    }
}

impl FromStr for EntryCategory {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "action" => Ok(Self::Action),
            "downloader" => Ok(Self::Downloader),
            "extractor" => Ok(Self::Extractor),
            "fixer" => Ok(Self::Fixer),
            other => Err(format!(
                "Unknown entry category {other:?}. Expected one of: action, downloader, extractor, fixer"
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, Hash, PartialEq)]
pub struct EntryId {
    pub category: EntryCategory,
    pub name: String,
}

impl EntryId {
    #[must_use]
    pub fn new(category: EntryCategory, name: impl AsRef<str>) -> Self {
        Self {
            category,
            name: name.as_ref().to_lowercase(),
        }
    }
}

impl FromStr for EntryId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (category, name) = s.split_once(':').ok_or_else(|| {
            format!("Invalid entry id. Expected `$CATEGORY:$NAME`, got {s:?}")
        })?;

        Ok(Self::new(category.parse()?, name))
    }
}

pub trait AsEntryId {
    fn entry_id(&self) -> EntryId;
}

impl<T> AsEntryId for &T
where
    T: AsEntryId + ?Sized,
{
    fn entry_id(&self) -> EntryId {
        (**self).entry_id()
    }
}

impl<T> AsEntryId for Arc<T>
where
    T: AsEntryId + ?Sized,
{
    fn entry_id(&self) -> EntryId {
        (**self).entry_id()
    }
}

impl<T> AsEntryId for Box<T>
where
    T: AsEntryId + ?Sized,
{
    fn entry_id(&self) -> EntryId {
        (**self).entry_id()
    }
}
```

- [ ] **Step 2: Rewrite `crates/app-config/src/common/disabled_entries.rs`** (delete `DisableEntry`, its `new`, the `From<(C, N)>` tuple impl, and its `FromStr`; keep the clap struct shape verbatim)

```rust
use clap::Args;
use entry_id::EntryId;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Default, Serialize, Deserialize, Args, Validate)]
#[clap(next_help_heading = Some("Action entries"))]
pub struct DisabledEntriesConfig {
    #[clap(
        long = "--disable-action-entry",
        value_delimiter = ',',
        env = "DOWNLOADER_HUB_DISABLED_ACTION_ENTRIES"
    )]
    pub entries: Vec<EntryId>,
}
```

In `crates/app-config/src/common/mod.rs` add `pub mod entry_id;` to the module list and `pub use entry_id::*;` to the re-export list.

- [ ] **Step 3: Port `crates/app-actions/src/config/mod.rs`**

Change imports: `common::{EndpointConfig, EntryCategory, EntryId, ProgramPathConfig, ProjectConfig, RequestConfig}` (drop `DisableEntry`). In `ActionsConfig`:

- field `pub disabled_entries: HashSet<DisableEntry>` -> `pub disabled_entries: HashSet<EntryId>`
- method `is_enabled` becomes:

```rust
    pub fn is_enabled(&self, entry: EntryId) -> bool {
        !self.disabled_entries.contains(&entry)
    }
```

- `init` signature loses its generics (no more tuple support - callers pass `Vec<EntryId>`):

```rust
pub fn init(
    endpoint: EndpointConfig,
    dependency_paths: ProgramPathConfig,
    disabled_entries: Vec<EntryId>,
    request: RequestConfig,
) -> Result<(), String> {
    let _ = app_helpers::config::init(dependency_paths.clone());

    ActionsConfig::init(ActionsConfig {
        endpoint,
        dependency_paths,
        request,
        disabled_entries: disabled_entries.into_iter().collect(),
    })?;

    Ok(())
}
```

- [ ] **Step 4: Port the four trait defaults and add `AsEntryId` impls**

In `crates/app-actions/src/actions/mod.rs` replace the `is_enabled` default body (the `ActionsConfig::global().is_enabled(("action", self.name()))` line) with `ActionsConfig::global().is_enabled(EntryId::new(EntryCategory::Action, self.name()))` and append after the trait:

```rust
impl app_config::AsEntryId for dyn Action {
    fn entry_id(&self) -> EntryId {
        EntryId::new(EntryCategory::Action, self.name())
    }
}
```

Same pattern in `src/downloaders/mod.rs` (`EntryCategory::Downloader`, `for dyn Downloader`), `src/extractors/mod.rs` (`EntryCategory::Extractor`, `for dyn Extractor + Sync + Send` - matching the `ExtractorEntry` alias), `src/fixers/mod.rs` (`EntryCategory::Fixer`, `for dyn Fixer + Send + Sync` - matching `FixerInstance`). Each file imports `EntryCategory, EntryId` from `app_config`.

- [ ] **Step 5: Port `bins/downloader-cli/src/config/mod.rs` literals**

In the `no_auto_crop` block replace `common::DisableEntry::new("fixer", "CropImage")` / `("fixer", "CropVideoBars")` with `common::EntryId::new(common::EntryCategory::Fixer, "CropImage")` / `(..., "CropVideoBars")`. No other change to this file in this task.

- [ ] **Step 6: Verify**

Run: `just fmt-dev`
Expected: clippy passes workspace-wide, formatting applied.

Run: `just dev-build app-config` and `just dev-build app-actions` and `just dev-build downloader-cli` and `just dev-build downloader-worker`
Expected: all compile. (Worker passes `config.disabled_entries.entries` - now `Vec<EntryId>` - into the new `init` signature unchanged.)

- [ ] **Step 7: Commit**

```bash
git add crates/app-config/src/common/entry_id.rs crates/app-config/src/common/disabled_entries.rs crates/app-config/src/common/mod.rs crates/app-actions/src/config/mod.rs crates/app-actions/src/actions/mod.rs crates/app-actions/src/downloaders/mod.rs crates/app-actions/src/extractors/mod.rs crates/app-actions/src/fixers/mod.rs bins/downloader-cli/src/config/mod.rs
git commit -m "Replace stringly DisableEntry with typed EntryId"
```

---

## Stage 2 - Bootstrap dedup

### Task 2: `BootConfig` trait; five bins adopt it

**Files:**
- Create: `crates/app-config/src/traits/boot_config.rs`
- Modify: `crates/app-config/src/traits/mod.rs` (add `pub mod boot_config;` + `pub use boot_config::*;`)
- Modify: `bins/downloader-central/src/config/mod.rs`, `bins/downloader-worker/src/config/mod.rs`, `bins/downloader-bot/src/config/mod.rs`, `bins/downloader-admin/src/config/mod.rs`, `bins/downloader-cli/src/config/mod.rs`

**Interfaces:**
- Produces: `trait BootConfig: clap::Parser + validator::Validate + Dumpable + Sized` with `fn resolve_paths(self) -> Self { self }`, `fn parse_validated() -> Self` (parse -> resolve_paths -> validate-or-exit(1)), `fn init_parsed() -> Result<Self, String>` (parse_validated -> `dump_if_needed`). Later tasks replace per-bin `init_parsed` shims with this trait method.

- [ ] **Step 1: Create `crates/app-config/src/traits/boot_config.rs`**

```rust
use clap::Parser;
use validator::Validate;

use crate::{Dumpable, validators::print_validation_errors};

pub trait BootConfig: Parser + Validate + Dumpable + Sized {
    fn resolve_paths(self) -> Self {
        self
    }

    fn parse_validated() -> Self {
        let parsed = Self::parse().resolve_paths();

        if let Err(e) = parsed.validate() {
            eprintln!("Errors validating configuration:");
            print_validation_errors(&e, "  ", 1);
            std::process::exit(1);
        }

        parsed
    }

    fn init_parsed() -> Result<Self, String> {
        Ok(Self::parse_validated().dump_if_needed())
    }
}
```

Add to `crates/app-config/src/traits/mod.rs`: `pub mod boot_config;` and extend the `pub use` to include `boot_config::*`. (`lib.rs` already does `pub use traits::*`.)

- [ ] **Step 2: Port central, worker, bot, admin**

In each of the four bins' `src/config/mod.rs`:

1. Import `BootConfig` in the `app_config` use list.
2. Delete the private `resolve_paths` and `validate_or_exit` methods.
3. Delete the inherent `init_parsed` body and replace with a shim that keeps main.rs untouched until Task 10:

```rust
impl BootConfig for Config {
    fn resolve_paths(mut self) -> Self {
        self.cmd = self.cmd.resolve_paths();
        self
    }
}

impl Config {
    pub fn init_parsed() -> Result<&'static Self, String> {
        Self::init(<Self as BootConfig>::init_parsed()?)
    }
}
```

Bin variations: **bot** resolves `self.dependency_paths = self.dependency_paths.resolve_paths();` instead of `self.cmd`; **admin** currently has no `resolve_paths` - use the trait default (empty `impl BootConfig for Config {}`). Keep each bin's remaining inherent items (`build_date`, `app_version`, `APPLICATION_VERSION`, etc.) untouched. The bins still derive `GlobalConfig` in this task (removed in Task 10).

- [ ] **Step 3: Port CLI**

`bins/downloader-cli/src/config/mod.rs`: import `BootConfig`; delete the private `resolve_paths`/`validate_or_exit`; replace the inherent `init_parsed` with:

```rust
impl BootConfig for Config {
    fn resolve_paths(mut self) -> Self {
        self.dependency_paths = self.dependency_paths.resolve_paths();
        self
    }

    fn init_parsed() -> Result<Self, String> {
        let mut parsed = Self::parse_validated();

        if parsed.run.no_auto_crop {
            parsed.disabled_entries.entries.extend([
                common::EntryId::new(common::EntryCategory::Fixer, "CropImage"),
                common::EntryId::new(common::EntryCategory::Fixer, "CropVideoBars"),
            ]);
        }

        Ok(parsed.dump_if_needed())
    }
}

impl Config {
    pub fn init_parsed() -> Result<&'static Self, String> {
        let parsed = <Self as BootConfig>::init_parsed()?;

        let owned = parsed.clone();
        app_actions::config::init(
            owned.endpoint,
            owned.dependency_paths,
            owned.disabled_entries.entries,
            owned.request,
        )?;

        Self::init(parsed)
    }
}
```

This preserves the current behavior order (resolve -> validate -> `no_auto_crop` mutation -> dump -> actions init -> global init).

- [ ] **Step 4: Verify**

Run: `just fmt-dev`
Expected: passes.

Run: `just dev-build downloader-central` and `just dev-build downloader-worker` and `just dev-build downloader-bot` and `just dev-build downloader-admin` and `just dev-build downloader-cli`
Expected: all compile; behavior identical (no flag changes).

- [ ] **Step 5: Commit**

```bash
git add crates/app-config/src/traits/boot_config.rs crates/app-config/src/traits/mod.rs bins/downloader-central/src/config/mod.rs bins/downloader-worker/src/config/mod.rs bins/downloader-bot/src/config/mod.rs bins/downloader-admin/src/config/mod.rs bins/downloader-cli/src/config/mod.rs
git commit -m "Deduplicate bin config bootstrap into BootConfig trait"
```

---

## Stage 3 - app-tasks DI

### Task 3: `TaskRunner` takes config by value; delete `app_tasks::config`

**Files:**
- Modify: `crates/app-tasks/src/lib.rs`, `crates/app-tasks/src/cron/mod.rs`
- Delete: `crates/app-tasks/src/config/mod.rs` (and remove `pub mod config;`)
- Modify: `bins/downloader-worker/src/cmd/work/mod.rs`

**Interfaces:**
- Produces: `TaskRunner::run(conf: app_config::common::TaskConfig) -> Self` (async, associated fn).

- [ ] **Step 1: app-tasks changes**

`crates/app-tasks/src/lib.rs` becomes:

```rust
use tracing::error;

pub(crate) mod cron;

use app_config::common;

pub struct TaskRunner;

impl TaskRunner {
    pub async fn run(conf: common::TaskConfig) -> Self {
        if let Err(e) = tokio::task::spawn_blocking(move || cron::spawn(conf)).await {
            error!("Failed to spawn cron tasks: {e:?}");
        }

        Self
    }
}
```

In `crates/app-tasks/src/cron/mod.rs`: change signature to `pub fn spawn(conf: common::TaskConfig)` (import `app_config::common`), delete the `use crate::config::TaskConfig;`, and change the single global read `TaskConfig::global().yt_dlp_update_interval` to `conf.yt_dlp_update_interval`. Delete `src/config/mod.rs` entirely.

- [ ] **Step 2: worker adoption**

In `bins/downloader-worker/src/cmd/work/mod.rs` (`async_run`): delete the `_ = app_tasks::config::init(config.task);` line; hoist the task config before the other config fields are moved (place right after the `debug!(config = ?config, ...)` in `run` or at the top of `async_run`):

```rust
let task_config = config.task;
```

and change `tc.spawn(TaskRunner::run());` to `tc.spawn(TaskRunner::run(task_config));`.

- [ ] **Step 3: Verify and commit**

Run: `just fmt-dev` (expect pass), `just dev-build app-tasks` and `just dev-build downloader-worker` (expect compile).

```bash
git add crates/app-tasks/src/lib.rs crates/app-tasks/src/cron/mod.rs crates/app-tasks/src/config/mod.rs bins/downloader-worker/src/cmd/work/mod.rs
git commit -m "Pass task config into TaskRunner by value"
```

---

## Stage 4 - app-actions registry + app-helpers paths (the big switch)

> This task changes `app-actions` public API, so the crate and both consuming bins change in **one commit**. Work through the steps in order; the workspace only compiles again after Step 7.

### Task 4: `ActionCtx` + `Actions` registry; thread ctx through traits/handlers; delete `ActionsConfig` + `HelpersConfig`

**Files:**
- Create: `crates/app-actions/src/ctx.rs`, `crates/app-actions/src/registry.rs`
- Modify: `crates/app-actions/src/lib.rs`, `src/actions/mod.rs`, `src/downloaders/mod.rs`, `src/extractors/mod.rs`, `src/fixers/mod.rs`, `src/actions/handlers/mod.rs`, `src/downloaders/handlers/mod.rs`, `src/extractors/handlers/mod.rs`, `src/fixers/handlers/mod.rs`, all handler files and shared helpers listed in the inventory below, `src/extractors/common/extract_info_request.rs`, `src/fixers/common/crop_filter.rs`
- Delete: `crates/app-actions/src/config/mod.rs`
- Modify: `crates/app-helpers/src/lib.rs`, `src/ffprobe.rs`, `src/temp_file.rs`, `src/dirs.rs`
- Delete: `crates/app-helpers/src/config/mod.rs`
- Modify: `bins/downloader-worker/src/cmd/work/mod.rs`, `src/cmd/work/app/mod.rs`, `src/cmd/work/app/process.rs`, `src/cmd/work/app/helpers/mod.rs` (if it re-exports the request helper), `src/cmd/list/mod.rs`
- Modify: `bins/downloader-cli/src/config/mod.rs`, `bins/downloader-cli/src/main.rs`
- Modify: `bins/downloader-bot/src/main.rs` (delete the `app_helpers::config::init` line)

**Interfaces:**
- Consumes: `EntryId`/`EntryCategory`/`AsEntryId` (Task 1).
- Produces:
  - `app_actions::ActionCtx { endpoint: Arc<EndpointConfig>, dependency_paths: Arc<ProgramPathConfig>, request: Arc<RequestConfig> }` with `ActionCtx::new(endpoint, dependency_paths, request) -> Self`.
  - `app_actions::Actions` with `new(ctx: Arc<ActionCtx>, disabled_entries: Vec<EntryId>) -> Self`, `ctx() -> &ActionCtx`, `is_enabled(id: impl AsEntryId) -> bool`, `all_actions()/all_downloaders()/all_fixers()/all_extractors()`, `enabled_actions() -> Vec<ActionEntry>` (async), `enabled_downloaders() -> Vec<DownloaderEntry>`, `enabled_fixers() -> Vec<FixerInstance>` (async), `enabled_extractors()`, `first_available_extractor(&ExtractInfoRequest) -> Option<ExtractorEntry>` (async), `extract_info(&ExtractInfoRequest) -> Result<ExtractedInfo, String>` (async), `download_file<R: Into<ExtractInfoRequest> + Send + Sync + Debug>(request, download_dir: &Path) -> Vec<DownloaderReturn>` (async), `fix_file<R: Into<FixRequest> + Send + Sync + Debug>(request) -> FixerReturn` (async).
  - `app_helpers::ffprobe::{ffprobe, ffprobe_async}` gain a `paths: &ProgramPathConfig` first parameter; `ffprobe_config{,_async}` gain it after `config`.
  - Deleted: `app_actions::config` module, `app_helpers::config` module, `AVAILABLE_ACTIONS`/`AVAILABLE_DOWNLOADERS`/`AVAILABLE_FIXERS`/`AVAILABLE_EXTRACTORS` statics (replaced by `ALL_*` + `Actions`), trait-default `is_enabled` on all four traits, `ExtractInfoRequest::{extractors, available_extractors, first_available_extractor, extract_info, extract_info_with}` methods (moved to `Actions`), free fns `app_actions::{download_file, fix_file}`, `downloaders::download_file`, `downloaders::download_file_with`, `extractors::{extract_info, extract_info_with}` (all become `Actions` methods / ctx-taking fns as specified).

- [ ] **Step 1: Create `crates/app-actions/src/ctx.rs`**

```rust
use std::sync::Arc;

use app_config::common::{EndpointConfig, ProgramPathConfig, RequestConfig};

#[derive(Debug, Clone)]
pub struct ActionCtx {
    pub endpoint: Arc<EndpointConfig>,
    pub dependency_paths: Arc<ProgramPathConfig>,
    pub request: Arc<RequestConfig>,
}

impl ActionCtx {
    #[must_use]
    pub fn new(
        endpoint: EndpointConfig,
        dependency_paths: ProgramPathConfig,
        request: RequestConfig,
    ) -> Self {
        Self {
            endpoint: Arc::new(endpoint),
            dependency_paths: Arc::new(dependency_paths),
            request: Arc::new(request),
        }
    }
}
```

- [ ] **Step 2: Create `crates/app-actions/src/registry.rs`**

```rust
use std::{collections::HashSet, path::Path, sync::Arc};

use app_config::{AsEntryId, EntryId};
use tracing::trace;

use crate::{
    ActionCtx,
    actions::{ActionEntry, handlers::ALL_ACTIONS},
    downloaders::{DownloadRequest, DownloaderEntry, DownloaderReturn, handlers::ALL_DOWNLOADERS},
    extractors::{ExtractedInfo, ExtractInfoRequest, ExtractorEntry, handlers::ALL_EXTRACTORS},
    fixers::{FixerInstance, FixRequest, handlers::ALL_FIXERS},
};

#[derive(Debug, Clone)]
pub struct Actions {
    ctx: Arc<ActionCtx>,
    disabled: HashSet<EntryId>,
}

impl Actions {
    #[must_use]
    pub fn new(ctx: Arc<ActionCtx>, disabled_entries: Vec<EntryId>) -> Self {
        Self {
            ctx,
            disabled: disabled_entries.into_iter().collect(),
        }
    }

    #[must_use]
    pub fn ctx(&self) -> &ActionCtx {
        &self.ctx
    }

    pub fn is_enabled(&self, id: impl AsEntryId) -> bool {
        !self.disabled.contains(&id.entry_id())
    }

    #[must_use]
    pub fn all_actions(&self) -> impl Iterator<Item = &ActionEntry> {
        ALL_ACTIONS.iter()
    }

    #[must_use]
    pub fn all_downloaders(&self) -> impl Iterator<Item = &DownloaderEntry> {
        ALL_DOWNLOADERS.iter()
    }

    #[must_use]
    pub fn all_fixers(&self) -> impl Iterator<Item = &FixerInstance> {
        ALL_FIXERS.iter()
    }

    #[must_use]
    pub fn all_extractors(&self) -> impl Iterator<Item = &ExtractorEntry> {
        ALL_EXTRACTORS.iter()
    }

    pub async fn enabled_actions(&self) -> Vec<ActionEntry> {
        let mut enabled = Vec::new();
        for action in ALL_ACTIONS.iter() {
            if !self.is_enabled(action.as_ref()) {
                trace!(?action, "Action is disabled");
                continue;
            }
            if action.can_run(&self.ctx).await {
                enabled.push(action.clone());
            }
        }
        enabled
    }

    #[must_use]
    pub fn enabled_downloaders(&self) -> Vec<DownloaderEntry> {
        ALL_DOWNLOADERS
            .iter()
            .filter(|x| self.is_enabled(x.as_ref()))
            .cloned()
            .collect()
    }

    pub async fn enabled_fixers(&self) -> Vec<FixerInstance> {
        let mut enabled = Vec::new();
        for fixer in ALL_FIXERS.iter() {
            if !self.is_enabled(fixer.as_ref()) {
                trace!(?fixer, "Fixer is disabled");
                continue;
            }
            if fixer.can_run(&self.ctx).await {
                enabled.push(fixer.clone());
            }
        }
        enabled
    }

    pub fn enabled_extractors(&self) -> impl Iterator<Item = &ExtractorEntry> {
        ALL_EXTRACTORS.iter().filter(|x| self.is_enabled(x.as_ref()))
    }

    pub async fn first_available_extractor(
        &self,
        request: &ExtractInfoRequest,
    ) -> Option<ExtractorEntry> {
        for extractor in ALL_EXTRACTORS.iter() {
            if self.is_enabled(extractor.as_ref()) && extractor.can_handle(request).await {
                return Some(extractor.clone());
            }
        }
        None
    }

    pub async fn extract_info(
        &self,
        request: &ExtractInfoRequest,
    ) -> Result<ExtractedInfo, String> {
        let extractor = self
            .first_available_extractor(request)
            .await
            .ok_or_else(|| extractors_no_extractor_error(request))?;
        crate::extractors::extract_info_with(&self.ctx, request, extractor.clone()).await
    }

    pub async fn download_file<R>(
        &self,
        request: R,
        download_dir: &Path,
    ) -> Vec<DownloaderReturn>
    where
        R: Into<ExtractInfoRequest> + Send + Sync + std::fmt::Debug,
    {
        let request = request.into();

        tracing::debug!(?request, "Extracting info");

        let info = match self.extract_info(&request).await {
            Ok(x) => x,
            Err(e) => {
                return vec![Err(crate::downloaders::DownloaderError::FallibleFailed(
                    format!("Failed to extract info from {request:?}: <u>{e}</u>"),
                ))];
            }
        };

        tracing::debug!(?info, "Extracted info");

        let download_requests =
            DownloadRequest::from_extracted_info(&info, download_dir);

        tracing::debug!(?download_requests, "Download requests");

        let downloaders = self.enabled_downloaders();
        let mut results = Vec::new();
        for req in download_requests {
            results.push(
                crate::downloaders::download_file_with(&self.ctx, &downloaders, &req).await,
            );
        }

        tracing::debug!(?results, "Download results");
        results
    }

    pub async fn fix_file<R>(&self, request: R) -> crate::fixers::FixerReturn
    where
        R: Into<FixRequest> + Send + Sync + std::fmt::Debug,
    {
        let fixers = self.enabled_fixers().await;
        crate::fixers::fix_file_with(&self.ctx, fixers, request.into()).await
    }
}

fn extractors_no_extractor_error(request: &ExtractInfoRequest) -> String {
    format!("Could not find an extractor that can handle {request:?}")
}
```

Notes: before finalizing, open the current `src/extractors/mod.rs` `extract_info`/`extract_info_with` and `src/lib.rs` `download_file` bodies and carry over their exact tracing spans (`info_span!("download_file", ...)`, `#[tracing::instrument(...)]` on `extract_info_with`) and error message strings so log output is unchanged. Adjust imports in this file to whatever the crate actually exports (the listing above follows current module paths; e.g. `DownloadRequest` is re-exported from `downloaders`).

- [ ] **Step 3: Rewire the four trait definitions** (verbatim new forms)

`src/actions/mod.rs`:

```rust
#[async_trait::async_trait]
#[typetag::serde(tag = "$action")]
pub trait Action: Debug + Send + Sync {
    fn name(&self) -> &'static str {
        self.typetag_name()
    }

    fn description(&self) -> &'static str;

    async fn can_run(&self, _ctx: &crate::ActionCtx) -> bool {
        true
    }

    async fn can_run_for(&self, _req: &ActionRequest) -> bool {
        true
    }

    async fn run(
        &self,
        ctx: &crate::ActionCtx,
        req: &ActionRequest,
    ) -> Result<ActionResult, ActionError>;
}
```

Keep the `impl AsEntryId for dyn Action` from Task 1. Delete the `is_enabled` default and the `use crate::config::ActionsConfig;`.

`src/downloaders/mod.rs` trait keeps `name`/`description`; delete `is_enabled`; `download` becomes `async fn download(&self, ctx: &crate::ActionCtx, req: &DownloadRequest) -> DownloaderReturn;`. Delete `download_file` (the `AVAILABLE_DOWNLOADERS`-driven free fn); change `download_file_with` to take ctx:

```rust
pub async fn download_file_with(
    ctx: &crate::ActionCtx,
    downloaders: &[DownloaderEntry],
    request: &DownloadRequest,
) -> DownloaderReturn
```

(body unchanged apart from `downloader.download(ctx, request).await`).

`src/extractors/mod.rs`: delete `is_enabled` default; `extract_info` becomes `async fn extract_info(&self, ctx: &crate::ActionCtx, request: &ExtractInfoRequest) -> Result<ExtractedInfo, String>;`. Delete the five `impl ExtractInfoRequest` methods (`extractors`, `available_extractors`, `first_available_extractor`, `extract_info`, `extract_info_with`). Keep/adjust the free `extract_info_with` as:

```rust
#[tracing::instrument(skip_all, fields(extractor = %extractor.name()))]
pub async fn extract_info_with(
    ctx: &crate::ActionCtx,
    request: &ExtractInfoRequest,
    extractor: handlers::ExtractorEntry,
) -> Result<ExtractedInfo, String>
```

(carry over its current body verbatim, adding `ctx` to the `extractor.extract_info(ctx, request)` call). Delete the old free `extract_info` (replaced by `Actions::extract_info`).

`src/fixers/mod.rs`: delete `is_enabled`; `can_run` gains `_ctx`; `run` gains `ctx`:

```rust
    async fn can_run(&self, _ctx: &crate::ActionCtx) -> bool {
        true
    }

    #[allow(unused_variables)]
    async fn can_run_for(&self, request: &FixRequest) -> bool {
        true
    }

    async fn run(&self, ctx: &crate::ActionCtx, request: &FixRequest) -> FixerReturn;
```

`fix_file_with` becomes `pub async fn fix_file_with(ctx: &crate::ActionCtx, fixers: Vec<FixerInstance>, request: FixRequest) -> FixerReturn` (body passes `ctx` to `fixer.run(ctx, &req)`); delete the old `fix_file` free fn.

- [ ] **Step 4: Rewire the registries** (`src/{actions,downloaders,extractors,fixers}/handlers/mod.rs`)

- Delete the `AVAILABLE_*` statics and the `available_*` functions in all four files. Keep `ALL_*` and `all_*()` verbatim. In `src/extractors/handlers/mod.rs` the `available_extractors()` free fn is deleted; `ALL_EXTRACTORS` is new:

```rust
pub static ALL_EXTRACTORS: LazyLock<Vec<ExtractorEntry>> = LazyLock::new(all_extractors);

#[must_use]
pub fn all_extractors() -> Vec<ExtractorEntry> {
    vec![
        // ...exact current list from available_extractors(), unchanged order...
    ]
}
```

- Rename the `pub use handlers::AVAILABLE_ACTIONS;`-style re-exports in the family `mod.rs` files to `pub use handlers::ALL_ACTIONS;` etc.

- [ ] **Step 5: Thread `ctx` through handlers - exhaustive inventory**

Universal substitution rules inside `crates/app-actions/src/`:

| Old | New |
|---|---|
| `ActionsConfig::endpoints()` | `&ctx.endpoint` |
| `ActionsConfig::request()` | `&ctx.request` |
| `ActionsConfig::dependency_paths()` | `&ctx.dependency_paths` |
| `ActionsConfig::cache_dir()` | `ProjectConfig::cache_dir()` (import from `app_config::common`) |

Signatures gaining `ctx: &ActionCtx` (add as **first** param after `self` where applicable):

- actions: `actions/handlers/ocr_image.rs` (`can_run`, `run`; reads `.endpoint.ocr_api_base_url` / `.endpoint.ocr_api_url(..)` at current lines 37/80/130), `actions/handlers/split_scenes.rs` (`can_run`, `run`; free fn `split_into_scenes(ctx, ...)` reads `.dependency_paths.scenedetect_path()`), `actions/handlers/remove_background.rs` (`run`; the `CropImage.run(&FixRequest::new(...))` call at ~line 135 becomes `CropImage.run(ctx, &FixRequest::new(...))`). `compact_media.rs`, `file_rename_to_id.rs` need only the trait-method signature update (no config reads); their `can_run_for`/`run` declarations gain `ctx` because the trait requires it.
- downloaders: `downloaders/handlers/yt_dlp.rs` (`download`; inherent `download_one(ctx, request)` reads `.dependency_paths.yt_dlp_path()` and `.request.user_agent`); `generic.rs`, `music/mod.rs` - signature updates only (their bodies don't read config).
- extractors: `extractors/handlers/instagram.rs` (`extract_info`; free fns `get_media_urls_authed(ctx, ...)` / `get_media_urls_anonymous(ctx, ...)` read `.request.user_agent`), `extractors/handlers/tiktok.rs` (`extract_info`; free fns `fetch_page_at(ctx, ...)`, `download_info(ctx, ...)` read `.request.user_agent`), `extractors/handlers/twitter.rs` (`extract_info`; inherent `screenshot_tweet_url(ctx, url)` and `screenshot_tweet_url_info(ctx, url)` read `.endpoint.twitter_screenshot_base_url`). Cross-family callers of `screenshot_tweet_url_info` (`bsky.rs:37`, `tumblr.rs:28`, `activity_pub/mastodon.rs:46`, `activity_pub/misskey.rs:44`) already have `ctx` in scope from their own `extract_info` - pass it through. All remaining extractor files: signature updates only.
- fixers: `fixers/handlers/crop_image.rs` (`can_run`, `run` read `.dependency_paths.imagemagick_path()`), `fixers/handlers/crop_video_bars.rs` (`can_run`, `run`; free fns `do_auto_crop_video(ctx, ...)`/`generate_crop_filter(ctx, ...)` read `.dependency_paths.ffmpeg_path()`), `fixers/handlers/media_formats.rs` (`run`; free fn `transcode_media_into(ctx: Arc<ActionCtx>, ...)` reads `.dependency_paths.ffmpeg_path()` and `ProjectConfig::cache_dir()`). In `media_formats.rs` the `CODEC_HANDLERS` const table of fn pointers changes element type from `fn(FfProbeResult, Stream) -> BoxFuture<'static, ...>` to `fn(Arc<ActionCtx>, FfProbeResult, Stream) -> BoxFuture<'static, ...>` and each dispatch site clones the ctx (`ctx.clone()`) before building the future. `file_extensions.rs`, `file_name.rs`: signature updates only.
- shared: `extractors/common/extract_info_request.rs` - `as_request_builder(&self, ctx: &ActionCtx)`; the user-agent read becomes `ctx.request.user_agent.as_str()`. Callers: `imgur.rs` (`get_post_data` - gains ctx), `tiktok.rs` (`fetch_page` - gains ctx). `fixers/common/crop_filter.rs` - `CropFilter::from_image_files(ctx, files)`, `generate_crop_filter_for_files(ctx, files, initial)`; the imagemagick read becomes `ctx.dependency_paths.imagemagick_path().expect("Imagemagick not found")`. Callers: `crop_image.rs:57`, `crop_video_bars.rs:226`.
- Delete `use crate::config::ActionsConfig;` (and equivalents) from every file that had one.

- [ ] **Step 6: `app-actions/src/lib.rs` and delete the config module**

New top of `lib.rs`:

```rust
pub mod actions;
pub(crate) mod common;
pub mod ctx;
pub mod downloaders;
pub mod extractors;
pub mod fixers;
pub mod registry;

pub use ctx::ActionCtx;
pub use registry::Actions;
```

Delete the old free `download_file`/`fix_file` fns and `pub mod config;`. Delete `crates/app-actions/src/config/mod.rs` and remove the module from `Cargo.toml` only if it lists modules (it does not - modules are file-driven).

- [ ] **Step 7: app-helpers paths**

`crates/app-helpers/src/ffprobe.rs`: add `paths: &ProgramPathConfig` as first param of `ffprobe`, `ffprobe_async`, and after `config` in `ffprobe_config`, `ffprobe_config_async`; replace both `HelpersConfig::dependency_paths().ffprobe_path()` reads with `paths.ffprobe_path()`. Import `app_config::common::ProgramPathConfig`.

`src/temp_file.rs` and `src/dirs.rs`: replace `HelpersConfig::cache_dir()` with `ProjectConfig::cache_dir()` (import from `app_config::common`).

Delete `crates/app-helpers/src/config/mod.rs`; remove `pub mod config;` from `crates/app-helpers/src/lib.rs`.

Update the app-actions ffprobe call sites: `fixers/handlers/crop_video_bars.rs` (`can_run_for`, `get_video_stream`) and `fixers/handlers/media_formats.rs` (`run`) call `ffprobe::ffprobe_async(&ctx.dependency_paths, path)`.

- [ ] **Step 8: worker adoption**

`bins/downloader-worker/src/cmd/work/mod.rs` (`async_run`): delete the three `_ = app_*::config::init(...)` lines; build the registry right after `let conf = ...`:

```rust
let actions = std::sync::Arc::new(app_actions::Actions::new(
    std::sync::Arc::new(app_actions::ActionCtx::new(
        config.endpoint,
        config.dependency_paths.clone(),
        config.request,
    )),
    config.disabled_entries.entries.clone(),
));
```

(Match the actual field moves: `dependency_paths` is needed by nothing else after this; `disabled_entries.entries` is not used later either - prefer moving over cloning where the original code moved.) `init_peering_endpoint(config.peer)` unchanged in this task. The tag-cleanup loop keeps `PeeringEndpoint::global()` until Task 6.

`src/cmd/work/app/mod.rs`:

- `run` signature: `pub async fn run(actions: Arc<app_actions::Actions>, config: PeerCommsWorkerTicketFromApiConfig, central_addr: IrohEndpointAddr) -> CmdResult`.
- `Capabilities::Worker` construction becomes:

```rust
let capabilities = Capabilities::Worker {
    extractors: actions
        .enabled_extractors()
        .map(|e| HandlerEntry {
            name: e.name().to_string(),
            description: e.description().to_string(),
        })
        .collect(),
    downloaders: actions
        .enabled_downloaders()
        .iter()
        .map(|d| HandlerEntry {
            name: d.name().to_string(),
            description: d.description().to_string(),
        })
        .collect(),
    fixers: actions
        .all_fixers()
        .filter(|f| actions.is_enabled(f.as_ref()))
        .map(|f| HandlerEntry {
            name: f.name().to_string(),
            description: f.description().to_string(),
        })
        .collect(),
};
```

(Old code filtered only extractors by enabled state; advertising uniformly-enabled handlers is the intended small behavior change.)

- `can_process_download_and_fix`: `req.first_available_extractor().await` -> `actions.first_available_extractor(&req).await` (thread `actions: &Actions` into `can_process`).
- The getWorkItem loop: `process::process_work_request(work_request)` -> `process::process_work_request(actions.clone(), work_request)`.
- `work/mod.rs` keep_running closure: capture `actions.clone()` alongside `conf.clone()` and pass to `app::run`.

`src/cmd/work/app/process.rs`: `process_work_request(actions: Arc<Actions>, work_request: WorkRequest)` - thread `actions` down through `process_download_and_fix` -> `download_and_fix` -> `fix_stage_and_deliver`. Call-site changes: `extractors::extract_info(&req)` -> `actions.extract_info(&req)`, `download_file(&x)` -> `actions.download_file(x, dir)` (match current call shapes), `app_actions::fix_file(path)` -> `actions.fix_file(path)`, `fix_stage_and_deliver`'s `app_actions::fix_file` likewise.

`src/cmd/list/mod.rs`: delete the `app_actions::config::init(...)` call; `run(config: ListConfig)` builds:

```rust
let actions = app_actions::Actions::new(
    std::sync::Arc::new(app_actions::ActionCtx::new(
        config.endpoint,
        config.dependency_paths,
        config.request,
    )),
    config.disabled_entries.entries,
);
```

and `list_actions`/`list_downloaders`/`list_fixers`/`list_all` take `actions: &Actions` and iterate `actions.all_actions()` / `all_downloaders()` / `all_fixers()` (these keep listing everything, ignoring disabled state - same as today).

- [ ] **Step 9: cli adoption**

`bins/downloader-cli/src/config/mod.rs`: delete the `app_actions::config::init` block from the inherent `init_parsed` shim (keep only `Self::init(parsed)`); `main.rs` builds `Actions` after `init_parsed`:

```rust
let config = Config::init_parsed().expect("Failed to init config");
let actions = app_actions::Actions::new(
    std::sync::Arc::new(app_actions::ActionCtx::new(
        config.endpoint.clone(),
        config.dependency_paths.clone(),
        config.request.clone(),
    )),
    config.disabled_entries.entries.clone(),
);
```

Call-site changes in `main.rs`: `download_file(request, &cli_config.output_directory)` -> `actions.download_file(request, &cli_config.output_directory)`; `fix_file(&x)` -> `actions.fix_file(&x)`; `RenameToId.run(&req)` -> `RenameToId.run(actions.ctx(), &req)`; `SplitScenes.run(&req)` -> `SplitScenes.run(actions.ctx(), &req)`. Remove the now-unused `app_actions::{download_file, fix_file}` imports.

`bins/downloader-bot/src/main.rs`: delete the `let _ = app_helpers::config::init(config.dependency_paths.clone());` line (the bot's remaining `app_helpers` uses are config-free).

- [ ] **Step 10: Verify**

Run: `just fmt-dev` (expect pass - fix any missed `ActionsConfig` references it surfaces).
Run: `just dev-build app-actions` `just dev-build app-helpers` `just dev-build downloader-worker` `just dev-build downloader-cli` `just dev-build downloader-bot`.
Manual smoke (optional but recommended): `cargo run -p downloader-cli -- --disable-action-entry fixer:CropImage -d /tmp/dl-test <some-url>` and `cargo run -p downloader-cli -- list all` - wait, `list` is a worker subcommand; CLI has no list. Instead: `just dev-run downloader-worker list all` requires env; skip if env is unavailable.

- [ ] **Step 11: Commit**

```bash
git add crates/app-actions crates/app-helpers bins/downloader-worker/src/cmd/work bins/downloader-worker/src/cmd/list bins/downloader-cli/src/config/mod.rs bins/downloader-cli/src/main.rs bins/downloader-bot/src/main.rs
git commit -m "Replace app-actions/app-helpers config globals with Actions registry"
```

---

## Stage 5 - app-database de-global + central state + admin db

### Task 5: `Arc<Database>` everywhere; `CentralState` in central; `db` in admin `AppState`

**Files:**
- Modify: `crates/app-database/src/client.rs`, `crates/app-database/src/error.rs` (drop `AlreadyInitialized`)
- Modify: `bins/downloader-central/src/cmd/central/mod.rs`, `src/cmd/central/components/mod.rs`, `src/cmd/central/components/rpc/mod.rs` (+ `distributor.rs`, `revocation.rs`, `restrictions.rs`, `session.rs` as needed), `src/cmd/central/components/database/mod.rs`, `src/cmd/central/components/worker_api/mod.rs`, `src/cmd/central/components/peers/mod.rs`
- Create: `bins/downloader-central/src/cmd/central/components/state.rs`
- Modify: `bins/downloader-admin/src/cmd/run/mod.rs`, `src/cmd/run/components/http_api/mod.rs`, `src/cmd/run/components/http_api/routes.rs`, `src/cmd/run/components/http_api/stream.rs`, `src/cmd/run/components/log_settings.rs`

**Interfaces:**
- Produces: `app_database::Database::new(cfg: DatabaseConfig) -> Result<Database, DatabaseError>` (already exists - becomes the only constructor; `Database::init`/`global`/`GLOBAL` deleted; `DatabaseError::AlreadyInitialized` deleted).
- Produces: `CentralState` (see code below) with `Arc<CentralState>` threaded to every central component and into `CentralRpcServer`.

- [ ] **Step 1: app-database**

In `client.rs`: delete `static GLOBAL`, `Database::global`, `Database::init`. In `error.rs`: delete the `AlreadyInitialized` variant (search the workspace for other references first; expect none).

- [ ] **Step 2: Create `CentralState`** (`components/state.rs`)

```rust
use std::{
    sync::{Arc, OnceLock},
    time::Instant,
};

use app_database::Database;
use app_peer_comms::{
    PeeringEndpoint,
    rpc::request::{CapabilitiesSummary, LogSettings, SecretEntry},
};
use arc_swap::ArcSwapOption;
use tokio::{sync::RwLock, task::JoinHandle};

use super::rpc::{RestrictionsManager, SessionRegistry, WorkDistributor};

#[derive(Default)]
pub struct CentralState {
    pub db: OnceLock<Arc<Database>>,
    pub sessions: SessionRegistry,
    pub distributor: ArcSwapOption<WorkDistributor>,
    pub initial_distributor_handle: OnceLock<JoinHandle<()>>,
    pub restrictions: ArcSwapOption<RestrictionsManager>,
    pub log_settings: RwLock<Vec<LogSettings>>,
    pub secrets: RwLock<Vec<SecretEntry>>,
    pub central_id: OnceLock<String>,
    pub peering: OnceLock<Arc<PeeringEndpoint>>,
}

pub type SharedCentralState = Arc<CentralState>;

impl CentralState {
    #[must_use]
    pub fn shared() -> SharedCentralState {
        Arc::new(Self::default())
    }

    pub fn db(&self) -> &Arc<Database> {
        self.db.get().expect("central database not initialized")
    }

    pub fn set_distributor(&self, handle: WorkDistributor, join: JoinHandle<()>) {
        self.distributor.store(Some(Arc::new(handle)));
        let _ = self.initial_distributor_handle.set(join);
    }

    pub fn distributor(&self) -> Arc<WorkDistributor> {
        self.distributor
            .load_full()
            .expect("work distributor not initialized")
    }

    pub fn respawn_distributor(&self) -> JoinHandle<()> {
        let (handle, join) = WorkDistributor::spawn(self.db().clone());
        self.set_distributor(handle, join);
        join
    }

    pub fn central_id(&self) -> String {
        self.central_id
            .get()
            .expect("central_id not initialized")
            .clone()
    }
}
```

Adjust to the real types: check whether `WorkDistributor::spawn()` currently takes no args (it does - it reads `Database::global()` internally; change it to `spawn(db: Arc<Database>)` in Step 4). If `SessionRegistry`/`RestrictionsManager`/`LogSettings` type paths differ, follow the actual imports in `rpc/mod.rs`. The `capabilities cache` statics (`CAPABILITIES_CACHE`, watch semaphores) **stay as statics** in `rpc/mod.rs` - they are memoization, not init-order state.

- [ ] **Step 3: central `async_run` and components**

`cmd/central/mod.rs`:

```rust
async fn async_run(config: CentralConfig) -> CmdResult {
    let state = components::state::CentralState::shared();

    let db = Arc::new(
        app_database::Database::new(config.database.clone())
            .await
            .expect("Failed to initialize database"),
    );
    let _ = state.db.set(db);

    let mut handles = components::spawn(config, state.clone()).in_current_span().await?;
    // ...existing join loop unchanged...

    if let Some(pe) = state.peering.get()
        && let Err(e) = pe.router.shutdown().await
    {
        warn!(?e, "Failed to shutdown peering router");
    }

    Ok(())
}
```

Delete the `use app_config::GlobalConfig;` import.

`components/mod.rs` `spawn(config: CentralConfig, state: SharedCentralState)`:

- Delete the five `rpc::init_*()` calls. Instead, at the top of `spawn`:

```rust
let _ = state.initial_distributor_handle; // populated by rpc bootstrap below
let (handle, join) = rpc::WorkDistributor::spawn(state.db().clone());
state.set_distributor(handle, join);
state.restrictions.store(Some(Arc::new(rpc::RestrictionsManager::new())));
```

- `init_peering(config.peer, state.clone())`: capture state in the router hook:

```rust
let hook_state = state.clone();
let pe = PeeringEndpoint::builder(config.common, topic_id)
    .with_router_hook(move |b| b.accept(RPC_ALPN, rpc::CentralRpcServer::new(hook_state.clone())))
    .build()
    .await?;
let pe = Arc::new(pe);
let _ = state.peering.set(pe.clone());
let _ = state.central_id.set(pe.endpoint_id().await.to_string());
Ok(())
```

(If `with_router_hook` requires `FnOnce`, drop the `move ... clone` dance and construct the server inline with a pre-cloned state captured once.)

- Component spawns: `database::run(state.clone())` (was `database::run(db_config.clone())`); `peers::run(state.clone())`; `worker_api::run(config.worker_api.clone(), state.clone())` (drop the `config.database` argument - it was only used to re-init the DB).

- `components/peers/mod.rs`: `run(state: SharedCentralState)`; `PeeringEndpoint::global()` -> `state.peering.get().expect("peering endpoint initialized")`.

- `components/database/mod.rs`: `run(state: SharedCentralState)`; delete the `Database::init` block; `Database::global()` (3 sites) -> `state.db()`; the `super::rpc::{take_initial_distributor_handle, respawn_distributor, distributor, set_log_settings, set_secrets}` calls become `state.{...}` method calls (`take_initial_distributor_handle` -> read `state.initial_distributor_handle.get().cloned()`; expose a small method if cleaner); the watcher fns (`run_log_settings_watcher`, `run_secrets_watcher`, `run_revocation_watcher`, `run_restrictions_watcher`) gain `state`/`db` params.

- `components/worker_api/mod.rs`: `run(config: WorkerHttpApiConfig, state: SharedCentralState)`; delete the `Database::init` call; any `Database::global()` -> `state.db()`.

- [ ] **Step 4: central rpc module**

In `components/rpc/mod.rs`:

- Delete statics `SESSIONS`, `DISTRIBUTOR`, `INITIAL_DISTRIBUTOR_HANDLE`, `CENTRAL_ID`, `LOG_SETTINGS`, `SECRETS`, `RESTRICTIONS` and the free fns `sessions`, `init_sessions`, `init_distributor`, `take_initial_distributor_handle`, `respawn_distributor`, `distributor`, `init_log_settings`, `set_log_settings`, `init_secrets`, `set_secrets`, `log_settings_for`, `init_central_id`, `central_id`, `init_restrictions`, `restrictions`.
- `CentralRpcServer { state: SharedCentralState }`; `new(state: SharedCentralState) -> Self`; delete the `Default` impl if it becomes unreachable. `ProtocolHandler` impl bodies replace the free-fn calls: `Database::global()` (27 sites) -> `self.state.db().as_ref()` (or bind `let db = self.state.db();` once per method), `distributor()` -> `self.state.distributor()`, `central_id()` -> `self.state.central_id()`, `restrictions()` -> `self.state.restrictions.load_full()`, `log_settings_for(&role)` -> `self.state.log_settings_for(&role)` - implement `log_settings_for` as a `CentralState` method carrying over the current body (`resolve_log_settings` + the `AuthedForRole::Admin => None` arm).
- `database/mod.rs` calls `set_log_settings(rows)`/`set_secrets(rows)` -> `*state.log_settings.write().await = rows;` (or small `CentralState::set_log_settings/set_secrets` methods - prefer the methods).
- `distributor.rs`: `WorkDistributor::spawn(db: Arc<Database>)` - replace its 3 internal `Database::global()` uses with the passed `db` (bind `let db = &self.db;` in the loop bodies as appropriate).
- `revocation.rs` / `restrictions.rs`: thread `db`/`state` params replacing their `Database::global()` uses; `run_revocation_watcher(db: Arc<Database>)` and `run_restrictions_watcher(state: SharedCentralState)` (check which statics they touch and thread accordingly).
- Watch spawn helpers in this file (`spawn_watch_mine_in_progress`, `spawn_watch_request`, `run_watch`, `send_request`) - thread `state` (they use `Database::global()` and `distributor()`).

- [ ] **Step 5: admin**

- `cmd/run/mod.rs` (`async_run`): `Database::init(config.database.clone()).await` -> `let db = Arc::new(Database::new(config.database.clone()).await?);` and pass `db.clone()` into `components::http_api::run(http_config, http_central_slot, session_secret, db.clone())` and `components::log_settings::run(db)`.
- `http_api/mod.rs`: `AppState` gains `pub db: Arc<Database>`; `run(...)` gains the `db: Arc<Database>` param and sets the field. (Import `app_database::Database`.)
- `routes.rs` (37 `Database::global()` sites) and `stream.rs` (6 sites): every `Database::global()` -> `state.db` (handlers already extract `State<AppState>`; in `stream.rs` use whatever state path the handlers have - if `LiveSnapshots` needs it, add a `db` field to `LiveSnapshots` set in `LiveSnapshots::spawn`... it must instead receive `db` from `run`: change `LiveSnapshots::spawn(db)`).
- `log_settings.rs`: `run(db: Arc<Database>)`; `Database::global().log_settings_watch()` -> `db.log_settings_watch()`.

- [ ] **Step 6: Verify**

Run: `just fmt-dev` (expect pass).
Run: `just dev-build app-database` `just dev-build downloader-central` `just dev-build downloader-admin`.

- [ ] **Step 7: Commit**

```bash
git add crates/app-database/src bins/downloader-central/src/cmd/central bins/downloader-admin/src/cmd/run
git commit -m "Replace Database global with Arc threading and CentralState"
```

---

## Stage 6 - peer-comms de-global + bins finish

### Task 6: `PeeringEndpoint` loses `GlobalConfig`; bins hold `Arc<PeeringEndpoint>`

**Files:**
- Modify: `crates/app-peer-comms/src/lib.rs`
- Modify: `bins/downloader-worker/src/cmd/work/mod.rs`, `bins/downloader-bot/src/peering/mod.rs`, `bins/downloader-bot/src/peering/rpc/mod.rs`, `bins/downloader-bot/src/peering/reconnect.rs`, `bins/downloader-admin/src/cmd/run/components/mod.rs`, `bins/downloader-central/src/cmd/central/components/mod.rs` (already done in Task 5 if you followed the `state.peering` step)

**Interfaces:**
- Produces: `PeeringEndpoint` as a plain struct (no `GlobalConfig` derive); `download_ticket_into(&self, ...)` instance method; `downloader()` reads an instance `OnceLock` field instead of a fn-local static. `PeeringEndpoint::global/init` deleted.

- [ ] **Step 1: app-peer-comms**

In `src/lib.rs`:

1. Remove `GlobalConfig` from the `#[derive(...)]` list on `PeeringEndpoint` (and the `use` of it).
2. `downloader()`: replace the fn-local `static DOWNLOADER: OnceLock<Downloader>` with a struct field `downloader: OnceLock<Downloader>` (add to the struct + set `OnceLock::new()` in the constructor `create()`); `pub fn downloader(&self) -> &Downloader { self.downloader.get_or_init(|| self.blobs.store().downloader(self.router.endpoint())) }`.
3. `download_ticket_into` (currently an associated fn using `Self::global()` three times around lib.rs:372-380): becomes `pub async fn download_ticket_into(&self, ticket: ..., file: ...) -> ...` with `Self::global()` -> `self`.
4. `#[derive(Debug, Clone, GlobalConfig)]` -> `#[derive(Debug, Clone)]`.

- [ ] **Step 2: worker**

`init_peering_endpoint` returns `(Arc<PeeringEndpoint>, IrohEndpointAddr)`; `PeeringEndpoint::init(pe)?` -> `let pe = Arc::new(pe);`. In `async_run`, bind `let peering = ...` and pass into: the tag-cleanup loop closure (`peering.delete_expired_tags()`), and `app::run(actions, peering.clone(), conf, central_addr)`. In `app/mod.rs` `run`: `RpcClient::init` call gains the endpoint - change worker `rpc.rs` in this task to `RpcClient::init(peering: &PeeringEndpoint, api_key, central_addr, capabilities)` where `let endpoint = peering.router.endpoint().clone();` replaces `PeeringEndpoint::global().router.endpoint().clone()`. (`RpcClient` keeps its static for now - Task 7 removes it.) `process.rs`: `PeeringEndpoint::global()` (~line 276) -> thread `peering: &Arc<PeeringEndpoint>` through `process_work_request` -> `fix_stage_and_deliver`; `PeeringEndpoint::download_ticket_into(...)` (~line 253) -> `peering.download_ticket_into(...)`.

- [ ] **Step 3: bot**

`peering/mod.rs` `init_peering_endpoint` returns `Arc<PeeringEndpoint>`: `PeeringEndpoint::init(pe)?` -> `Arc::new(pe)`; `RpcClient::init(peering.as_ref(), config.ticket.api.key.clone(), central_addr, capabilities)`. In `peering/rpc/mod.rs`: `connect_and_auth` gains `peering: &PeeringEndpoint` (replacing `PeeringEndpoint::global()`), `init` passes it through, `reauth` uses `Self::global().peering.clone()` - add a `peering: Arc<PeeringEndpoint>` field to `RpcClient` set in `init` (interim until Task 8). In `peering/reconnect.rs` nothing changes yet.

- [ ] **Step 4: admin**

`components/mod.rs` `establish_once`: build the endpoint once. Move the `PeeringEndpoint::builder(...).build().await` **out of** `establish_once` into `connect_central` before the loop (first successful build wins; subsequent reconnects reuse it - this matches current effective behavior where `init` fails and the existing instance is reused). Replace `PeeringEndpoint::init(pe)` + `PeeringEndpoint::global().router.endpoint()` with the owned `Arc<PeeringEndpoint>`; `connect_central(config, central_slot)` also receives `peering: Arc<PeeringEndpoint>` from `run/mod.rs`, which gets it from... the peering build now happens inside `connect_central`, so: `connect_central` owns the built `Arc<PeeringEndpoint>` locally. `cmd/run/mod.rs` unchanged apart from the call shape.

- [ ] **Step 5: Verify and commit**

Run: `just fmt-dev` (expect pass); `just dev-build app-peer-comms` `just dev-build downloader-worker` `just dev-build downloader-bot` `just dev-build downloader-admin` `just dev-build downloader-central`.

```bash
git add crates/app-peer-comms/src/lib.rs bins/downloader-worker/src/cmd/work bins/downloader-bot/src/peering bins/downloader-admin/src/cmd/run/components/mod.rs
git commit -m "Remove PeeringEndpoint global in favor of Arc threading"
```

### Task 7: worker finish - owned `RpcClient`, `Broadcaster`, secrets, heartbeat slot

**Files:**
- Modify: `bins/downloader-worker/src/cmd/work/rpc.rs`, `src/cmd/work/app/mod.rs`, `src/cmd/work/app/broadcaster/mod.rs`, `src/cmd/work/app/process.rs`, `src/cmd/work/mod.rs`

**Interfaces:**
- Produces: `RpcClient` owned value: `RpcClient::connect(peering: &PeeringEndpoint, api_key, central_addr, capabilities) -> Result<RpcClient, ...>` with instance methods `get_work_item`, `refuse_work_item`, `heartbeat`, `get_log_settings`, `get_secrets`, `work_request_*` (all `&self`). `WorkerLoop { rpc: Arc<ArcSwapOption<RpcClient>> }` shared slot owned by `async_run`. `Broadcaster::new(rpc: Arc<ArcSwapOption<RpcClient>>)` value. Secrets `Arc<RwLock<HashMap<String, SecretEntry>>>` owned by `WorkerLoop`.

- [ ] **Step 1: rpc.rs**

Delete `static RPC_CLIENT`, `init`, `global`, `client()`. New shape:

```rust
pub struct RpcClient {
    inner: ArcSwapOption<irpc::Client<CentralProtocol>>,
}

impl RpcClient {
    pub async fn connect(
        peering: &PeeringEndpoint,
        api_key: Arc<str>,
        central_addr: IrohEndpointAddr,
        capabilities: app_peer_comms::rpc::request::Capabilities,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let endpoint = peering.router.endpoint().clone();
        let client = irpc_iroh::client::<CentralProtocol>(endpoint, central_addr, RPC_ALPN);

        match client
            .rpc(request::Auth {
                api_key,
                capabilities,
                version: crate::config::Config::app_version().to_string(),
            })
            .await?
        {
            AuthResult::Ok(info) => tracing::info!(?info, "irpc session established"),
            AuthResult::Unauthorized => {
                tracing::error!(
                    "irpc authentication rejected; the API key is likely revoked or expired. \
                     Terminating."
                );
                std::process::exit(1);
            }
        }

        Ok(Self {
            inner: ArcSwapOption::from(Some(Arc::new(client))),
        })
    }

    fn client(&self) -> Arc<irpc::Client<CentralProtocol>> {
        self.inner
            .load_full()
            .expect("downloader-worker RPC client not connected")
    }
}
```

Every request method becomes `&self` (bodies unchanged: `Self::client()` -> `self.client()`).

- [ ] **Step 2: WorkerLoop + broadcaster + secrets**

In `app/mod.rs` (or a new small `app/loop_state.rs` if cleaner):

```rust
#[derive(Clone, Default)]
pub struct WorkerLoop {
    pub rpc: Arc<ArcSwapOption<rpc::RpcClient>>,
    secrets: Arc<std::sync::RwLock<HashMap<String, SecretEntry>>>,
}

impl WorkerLoop {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_rpc(&self, client: rpc::RpcClient) {
        self.rpc.store(Some(Arc::new(client)));
    }

    pub fn rpc(&self) -> Option<Arc<rpc::RpcClient>> {
        self.rpc.load_full()
    }

    pub fn set_secrets(&self, secrets: Vec<SecretEntry>) {
        let map = secrets.into_iter().map(|s| (s.name.clone(), s)).collect();
        *self.secrets.write().expect("secrets lock poisoned") = map;
    }

    pub fn secret_value_for(
        &self,
        url: &url::Url,
        ordered_by: Option<&AccountUserRef>,
        ordered_in: Option<&AccountPlaceRef>,
    ) -> Option<String> {
        let name = platform_secret_name(url.host_str()?)?;
        let guard = self.secrets.read().ok()?;
        let entry = guard.get(name)?;
        // ...user_ok/place_ok checks carried over verbatim...
    }
}
```

Delete `static SECRETS`, `set_secrets`, `secret_value_for` free fns and `static HEARTBEAT`. `refresh_dynamic_settings(loop: &WorkerLoop)` loads `loop.rpc()` and skips (with a `debug!`) when `None`.

`broadcaster/mod.rs`: delete `static BROADCASTER`, `init`, `get`; `Broadcaster::new(rpc: Arc<ArcSwapOption<RpcClient>>)` stores the slot; each `send_work_request_*` method is `&self` and its inner closure loads the client:

```rust
pub fn send_work_request_free(&self, request_id: Arc<str>) {
    let rpc = self.rpc.clone();
    spawn(deliver("work_request_free", move || {
        let id = request_id.clone();
        let rpc = rpc.load_full();
        async move {
            let Some(rpc) = rpc else {
                tracing::error!("work_request_free dropped: RPC client not connected");
                return Ok(());
            };
            rpc.work_request_free(id).await.map(drop)
        }
    }));
}
```

Apply the same pattern to all six senders.

- [ ] **Step 3: rewire run flow**

`app/mod.rs` `run(actions, peering, loop: WorkerLoop, config, central_addr)`:

1. `let rpc = rpc::RpcClient::connect(peering.as_ref(), config.key.clone(), central_addr, capabilities).await?;` then `loop.set_rpc(rpc);`
2. `let broadcaster = broadcaster::Broadcaster::new(loop.rpc.clone());`
3. `refresh_dynamic_settings(&loop).await;`
4. Heartbeat: spawned **once** in `async_run` (replacing the `HEARTBEAT` OnceLock):

```rust
tc.spawn({
    let loop_state = worker_loop.clone();
    async move {
        loop {
            let jitter = rand::random_range(0..5_000u64);
            tokio::time::sleep(Duration::from_millis(30_000 + jitter)).await;
            refresh_dynamic_settings(&loop_state).await;
        }
    }
});
```

5. The getWorkItem loop: `let Some(rpc) = loop.rpc() else { return Err("RPC client not connected".into()); };` once per `app::run` invocation, then `rpc.get_work_item().await` / `rpc.refuse_work_item(...)`; pass `broadcaster` + `loop` (for `secret_value_for`) into `process::process_work_request(actions.clone(), peering.clone(), broadcaster.clone(), loop_state.clone(), work_request)`.
6. `process.rs`: `Broadcaster::get().send_*` (20 sites) -> `broadcaster.send_*`; `crate::cmd::work::app::secret_value_for(...)` -> `loop_state.secret_value_for(...)`; thread both params down `process_download_and_fix` -> `download_and_fix` -> `fix_stage_and_deliver`.

`work/mod.rs`: `let worker_loop = app::WorkerLoop::new();` in `async_run`; capture into the keep_running closure.

- [ ] **Step 4: Verify and commit**

Run: `just fmt-dev`; `just dev-build downloader-worker`.

```bash
git add bins/downloader-worker/src/cmd/work
git commit -m "Own worker RpcClient/broadcaster/secrets in WorkerLoop instead of statics"
```

### Task 8: bot finish - platform runtimes via framework state

**Files:**
- Modify: `bins/downloader-bot/src/cmd/telegram/bot/mod.rs`, `src/cmd/telegram/bot/handlers/**`, `src/cmd/telegram/bot/helpers/**`, `src/cmd/telegram/mod.rs`
- Modify: `bins/downloader-bot/src/cmd/discord/bot/discord_bot.rs`, `src/cmd/discord/bot/mod.rs`, `src/cmd/discord/bot/handlers/**`, `src/cmd/discord/bot/helpers/**`, `src/cmd/discord/broadcaster/mod.rs`, `src/cmd/discord/mod.rs`
- Modify: `bins/downloader-bot/src/peering/mod.rs`, `src/peering/rpc/mod.rs`, `src/peering/reconnect.rs`, `src/main.rs`

**Interfaces:**
- Produces: `TelegramBot` owned value injected via teloxide `Dispatcher::dependencies` (`dptree::deps![Arc<TelegramBot>]`); all `TelegramBot::instance()`/static accessors become `&self` methods. `DiscordBot` owned value injected via serenity TypeMap key `DiscordBotKey`; `MessageBroadcaster` value owned by the serenity `Handler`. Bot `RpcClient` owned value with `reconnect(&self)`; `ReconnectCoordinator` instance owned by `RpcClient`.

- [ ] **Step 1: bot RpcClient ownership + reconnect**

`peering/rpc/mod.rs`: delete `static RPC_CLIENT`, `global`, `client()`; struct gains `peering: Arc<PeeringEndpoint>` and `coordinator: ReconnectCoordinator` and `api: PeerCommsBotTicketFromApiConfig` fields; `init` -> associated constructor `connect(peering: Arc<PeeringEndpoint>, api: PeerCommsBotTicketFromApiConfig, central_addr, capabilities) -> Result<Arc<Self>, ...>`; `reauth(&self, central_addr)` and every request method becomes `&self`; the retry-on-failure path in `work_request_create` calls `self.reconnect().await` instead of `crate::peering::reconnect().await`.

`peering/reconnect.rs`: convert to instance state:

```rust
#[derive(Default)]
pub struct ReconnectCoordinator {
    in_flight: std::sync::Mutex<Option<Arc<ReconnectAttempt>>>,
    connect_config: PeerCommsBotTicketFromApiConfig,
}
```

`reconnect` becomes a method on `RpcClient` (`pub async fn reconnect(&self)`) that uses `self.coordinator` and `self.api`; delete `static RECONNECT`, `static CONNECT_CONFIG`, `set_connect_config`, `connect_config()`. Carry over the single-flight logic verbatim (only the state access changes). External callers of `crate::peering::reconnect()` (telegram `startup_scan`'s `reconnect_and_backoff`, heartbeat in `peering/mod.rs`) receive an `Arc<RpcClient>` and call `rpc.reconnect().await`.

`peering/mod.rs`: `init_peering_endpoint(...) -> Result<Arc<rpc::RpcClient>, ...>` - builds endpoint, connects RpcClient, spawns the heartbeat task with the `Arc<RpcClient>` clone (delete `static HEARTBEAT` - the heartbeat is spawned once from `init_peering_endpoint`, which itself runs once per process), returns the client. `main.rs` `async_main`: `let rpc = peering::init_peering_endpoint(config.peer, capabilities).await?;` then `cmd::run(config.cmd, rpc).await`; `cmd/mod.rs` dispatch passes `rpc: Arc<peering::rpc::RpcClient>` into `telegram::run`/`discord::run`.

- [ ] **Step 2: Telegram**

`bot/mod.rs`: delete `static TELEGRAM_BOT`, `init`, `instance`. `TelegramBot` methods become instance methods:

```rust
impl TelegramBot {
    #[must_use]
    pub fn bot(&self) -> &teloxide::Bot { self.inner.inner().inner() }
    #[inline]
    #[must_use]
    pub fn max_payload_size(&self) -> Size { self.config.max_payload_size }
    #[must_use]
    pub fn effective_max_filesize(&self) -> Size { /* same body via self */ }
    #[must_use]
    pub fn owner_id(&self) -> Option<teloxide::types::UserId> { /* same */ }
    #[must_use]
    pub fn owner_download_dir(&self) -> Option<std::path::PathBuf> { /* same */ }
}
```

`TelegramBot::run(bot: Arc<TelegramBot>)` builds the dispatcher with dependencies:

```rust
let dispatcher = Dispatcher::builder(bot.inner.clone(), Update::filter_message().endpoint(answer))
    .dependencies(dptree::deps![bot.clone()])
    .build();
Box::pin(dispatcher.dispatch()).await;
```

`answer` becomes `async fn answer(_bot: &TeloxideBot, tg: Arc<TelegramBot>, msg: Message) -> ResponseResult<()>` (keep the bot parameter exactly as it is today - including its reference form - and add the injected `tg` parameter; dptree supplies it by type from `dependencies`) and passes `tg` into `handlers::command::handle_command(tg, &msg, c)` / `handlers::message::handle_message(tg, &msg)`. Call-site conversions (thread `tg: Arc<TelegramBot>` as a first parameter through these files - inventory from the pre-task grep): `handlers/command/mod.rs` (`TelegramBot::instance().send_message` -> `tg.send_message`, `.config.clone()` -> `tg.config.clone()`), `handlers/message/mod.rs:28` (`TelegramBot::effective_max_filesize()` -> `tg.effective_max_filesize()`), `handlers/delivery.rs:34,64,93`, `helpers/status_message.rs:53,101,141,215` (StatusMessage methods gain a `tg: &Arc<TelegramBot>` parameter - or a `tg` field set at construction; pick the field if StatusMessage is constructed in few places), `helpers/account.rs:100,111`, `helpers/file_id.rs:28,50`. `telegram/mod.rs` `run`: `TelegramBot::init(config.bot)` -> `let bot = Arc::new(TelegramBot::new(config.bot));` then `bot::TelegramBot::run(bot.clone())`, and `startup_scan`/`recover_request` receive the `rpc`/`tg` handles they need (`startup_scan(rpc.clone())`; `start_request_task` gains `tg` param).

- [ ] **Step 3: Discord**

`bot/discord_bot.rs`: delete `static DISCORD_BOT`, `init`, `instance`; accessors become `&self` methods (`bot(&self) -> &Arc<Http>`, `owner_id(&self)`, `owner_download_dir(&self)`, `max_payload_size(&self)`, `configured_max_filesize(&self)`, `safe_max_filesize(&self)`; `destination_max_filesize` stays an associated const fn).

TypeMap key (in `discord_bot.rs`):

```rust
pub struct DiscordBotKey;

impl serenity::prelude::TypeMapKey for DiscordBotKey {
    type Value = Arc<DiscordBot>;
}
```

`broadcaster/mod.rs`: delete `static BROADCASTER`, `init`, `get`; keep `new()`, `send_message`, `try_send`, `recv`; `send` (associated) becomes `pub fn send_message(&self, ...)` at call sites. `cmd/discord/mod.rs` `run`:

```rust
let broadcaster = broadcaster::MessageBroadcaster::new();

let mut client = Client::builder(config.bot.bot_token, intents)
    .event_handler(bot::Handler::new(bot::HandlerConfig {
        about_text,
        broadcaster: broadcaster.clone(),
    }))
    .await?;

let discord_bot = Arc::new(DiscordBot {
    http: client.http.clone(),
    config: bot_config,
});
client.data.write().insert::<DiscordBotKey>(discord_bot);

// ...account refresh spawn unchanged...
client.start_autosharded().await
```

(Add `broadcaster: MessageBroadcaster` to `HandlerConfig`.) Handler methods retrieve the bot: `let bot = ctx.data.read().get::<DiscordBotKey>().cloned().expect("DiscordBot in TypeMap");` and pass `bot`/`broadcaster` down. Call-site conversions: `bot/mod.rs:126,148,210` (`MessageBroadcaster::get()/send` -> `self.broadcaster`/captured value), `bot/handlers/message.rs:169` (`DiscordBot::max_payload_size()...` -> `bot.max_payload_size()...`), `bot/handlers/delivery.rs:29,59,83`, `bot/helpers/account.rs:155,166`, `bot/helpers/status_message.rs:67,80` (StatusMessage gains a `bot` field or param, mirroring the telegram change).

- [ ] **Step 4: Verify and commit**

Run: `just fmt-dev`; `just dev-build downloader-bot`.

```bash
git add bins/downloader-bot/src
git commit -m "Inject bot runtimes via dispatcher/TypeMap instead of statics"
```

### Task 9: Delete `GlobalConfig` machinery; bins own their configs; AGENTS.md updates

**Files:**
- Modify: `bins/downloader-central/src/main.rs`, `bins/downloader-worker/src/main.rs`, `bins/downloader-bot/src/main.rs`, `bins/downloader-admin/src/main.rs`, `bins/downloader-cli/src/main.rs` (+ their `src/config/mod.rs`)
- Modify: `crates/app-macros/src/lib.rs`, `crates/app-config/src/traits/mod.rs`, `crates/app-config/src/lib.rs`
- Delete: `crates/app-config/src/traits/global_config.rs`
- Modify: `AGENTS.md` (root), `bins/*/AGENTS.md` (startup contracts)

**Interfaces:**
- Produces: bins' `Config` structs without `GlobalConfig` derive; `main` uses `BootConfig::init_parsed()` returning an owned `Config`. `GlobalConfig` trait + derive deleted from the workspace.

- [ ] **Step 1: bins**

In each bin's `src/config/mod.rs`: remove `GlobalConfig` from the derive list and the `use app_config::GlobalConfig;` import; delete the inherent `init_parsed` shim (Task 2). CLI keeps its custom `BootConfig::init_parsed` override and deletes its shim.

In each bin's `main.rs`: `let config = config::Config::init_parsed().expect("Failed to initialize config");` keeps working - it now resolves to the trait method and returns an owned `Config`. Fix the derefs: `trace!(config = ?*config, ...)` -> `trace!(config = ?config, ...)`; `cmd::run(config.cmd.clone())` -> move the field (`cmd::run(config.cmd)`) wherever the config isn't used afterwards (check each main's remaining field reads - log fields are read before `cmd::run`, so moving `cmd` is safe).

CLI `main.rs`: delete `Config::global()`/`Config::run()` uses - `get_explicit_urls(&config.run)`, `get_explicit_split_files(&config.run)`, `get_explicit_files(&config.run)` take `&RunConfig` params; `Config::run()` accessor deleted from `src/config/mod.rs`.

Central `cmd/central/mod.rs`: `use app_config::GlobalConfig;` - delete (if Task 5 didn't already).

- [ ] **Step 2: delete the machinery**

- `crates/app-macros/src/lib.rs`: delete the entire `global_config_derive` proc-macro (keep `dumpable_derive`).
- `crates/app-config/src/traits/global_config.rs`: delete file; remove `pub mod global_config;` + its re-export from `traits/mod.rs`; check `lib.rs` `pub use traits::*` needs no change.
- Workspace grep for `GlobalConfig` - expect zero hits outside git history.

- [ ] **Step 3: docs**

- Root `AGENTS.md`, replace the lint/style bullet that reads "Each crate owns its own `Config` (`pub(crate)`); initialize with `Config::init_parsed()` then read `Config::global()`." with: "Config is passed by value: bins parse once via `Config::init_parsed()` (the `BootConfig` trait) and thread `Arc`-wrapped values/handles down. There are no config globals; the only process globals left are `app-logger`'s tracing reload handles."
- `bins/downloader-central/AGENTS.md`: fix the boot sequence line to the real order: "crypto provider -> dotenv -> `Config::init_parsed()` -> `app_logger::init_with_options()` -> `cmd::run`"; update the startup contract to describe `CentralState` (`Arc<CentralState>` built in `async_run`, database `Arc` stored on it, peering endpoint + central id set during `init_peering`).
- `bins/downloader-worker/AGENTS.md`: replace the "must init app_tasks/app_helpers/app_actions config first" step with "build `Arc<Actions>` from `WorkerConfig` (ActionCtx + disabled entries) before peering; `WorkerLoop` owns the RPC slot/secrets".
- `bins/downloader-bot/AGENTS.md`: replace step 2 (`app_helpers::config::init`) - it no longer exists; describe `init_peering_endpoint` returning the owned `RpcClient` and platform runtimes injected via dispatcher deps / TypeMap.
- `bins/downloader-admin/AGENTS.md`: `Database::init` step becomes "`Arc::new(Database::new(...))` threaded into HTTP state and the log-settings watcher".
- `bins/downloader-cli/AGENTS.md`: mention building `Actions` in `main` (with the `--no-auto-crop` disabled-set extension) instead of the config-global side effect.

- [ ] **Step 4: Verify and commit**

Run: `just fmt-dev`; then build every bin: `just dev-build downloader-central` `just dev-build downloader-worker` `just dev-build downloader-bot` `just dev-build downloader-admin` `just dev-build downloader-cli`. Optional full release sanity: `just build downloader-worker` (slow; only if you want the canonical prod build check).

```bash
git add bins crates/app-macros/src/lib.rs crates/app-config/src AGENTS.md bins/downloader-central/AGENTS.md bins/downloader-worker/AGENTS.md bins/downloader-bot/AGENTS.md bins/downloader-admin/AGENTS.md bins/downloader-cli/AGENTS.md
git commit -m "Remove GlobalConfig machinery; bins own their configs"
```

---

## Rollback / risk notes

- Highest-risk tasks: **Task 4** (public API sweep) and **Task 5** (central rpc, 1400-line file). If a task's build fails partway, prefer finishing the task over reverting - the intermediate states intentionally do not compile.
- Tasks 4-6 preserve runtime behavior with two conscious exceptions: (1) capabilities advertised by the worker now uniformly exclude disabled handlers (extractors already did); (2) `Broadcaster` senders log-and-drop instead of panicking when the RPC client is absent.
- The dynamic-settings channels (log levels, secrets, per-request filesize) are untouched by design; verify with a `DOWNLOADER_HUB_LOG_LEVEL=debug just dev-run downloader-cli -- --dump-config json` style smoke where env allows.
