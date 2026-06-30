# Finish Discord Bot Implementation (Telegram Parity)

## TL;DR

The Discord bot at `bins/downloader-bot/src/cmd/discord/` has the infrastructure
(`Handler`, `MessageBroadcaster`, `BotCommand` enum) but none of the actual
download pipeline. The Telegram bot at `bins/downloader-bot/src/cmd/telegram/`
is the reference implementation. This plan brings the Discord bot to feature
parity with Telegram: real `DownloadAndFix` via RPC, free-form URL intake,
`process_work_request` pipeline (download → chunk → upload → mark complete),
with StatusMessage and file-grouping helpers.

---

## Background & Context

### Process architecture (from root `AGENTS.md`)

Four binaries share state via **Convex** and communicate via **iroh**:

- `downloader-central` — axum HTTP server, coordination point
- `downloader-worker` — performs downloads/processing
- `downloader-bot` — multi-platform bot (Telegram + Discord), selected via subcommand
- `downloader-cli` — local CLI tool

The bot's job is to:

1. Receive user requests (URLs or attached files) via chat.
2. Create **work requests** via RPC to central (`RpcClient::work_request_create`).
3. Watch for status updates on its in-progress work requests via WebSocket
   (`RpcClient::work_request_watch_mine_in_progress`).
4. When a work request finishes (`waiting_for_requester = true` with
   `files_data`), download the result files, upload them to the chat, and call
   `RpcClient::work_request_complete`.

Status updates from the bot to the chat (and back to the worker via Convex)
travel through `StatusMessage` metadata embedded in each work request.

### Discord bot's current state (pre-implementation)

| File                                 | Status                                                                                                                                                                          |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cmd/discord/bot/mod.rs`             | `EventHandler` skeleton; `BotCommand::DownloadAndFix` is a **placeholder** that replies with `hello.txt` containing JSON of URLs; `watch_work_requests()` only does `dbg!(req)` |
| `cmd/discord/broadcaster/mod.rs`     | **Complete** — global/channel/reply/edit/reaction/delete broadcasts with retry loop                                                                                             |
| `cmd/discord/bot/error_formatter.rs` | **Dead code** — module is commented out (`// mod error_formatter;`); also uses unstable `is_multiple_of` feature                                                                |
| `cmd/discord/config.rs`              | Thin wrapper around `DiscordBotConfig`                                                                                                                                          |
| `cmd/_common/`                       | **Empty** — intended shared home (per AGENTS.md hint, never used yet)                                                                                                           |

The Discord bot has no:

- URL intake from free-form messages
- StatusMessage helper (Telegram's status message concept)
- File-grouping helper (Discord's ≤10 attachments per message cap)
- `process_work_request` pipeline (download → upload → complete)
- Real work request creation via RPC
- Owner save-directory support

### Telegram bot — the reference

The Telegram implementation has all of the above. Key files (in
`bins/downloader-bot/src/cmd/telegram/`):

- `bot/mod.rs` — `TelegramBot` singleton (wraps teloxide bot, holds `Arc<TelegramBotConfig>`)
- `bot/handlers/command/mod.rs` — handles `/help`, `/start`, `/about`, `/ping`
- `bot/handlers/message/mod.rs` — `handle_message` (URL intake) + `process_work_request` (full pipeline)
- `bot/helpers/status_message.rs` — `StatusMessage` struct, serialized into work-request metadata
- `bot/helpers/file_group.rs` — chunks downloaded files into Telegram media groups (≤10 per group, size cap)
- `bot/helpers/file_id.rs` — extracts Telegram file IDs from messages (note: **not actually used for downloads** — only as a "has media" check)
- `common/downloadable/{mod.rs, impls/file_reference.rs}` — `Downloadable` trait + impl for `FileReference`

### Key differences between Telegram and Discord relevant to this work

| Concern            | Telegram                                                       | Discord                                                                          |
| ------------------ | -------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| Chat identifier    | `ChatId` (i64 wrapper)                                         | `ChannelId` (u64 wrapper)                                                        |
| Message identifier | `MessageId` (i32 wrapper)                                      | `MessageId` (u64 wrapper)                                                        |
| Reply target       | `ReplyParameters::new(msg.id)`                                 | `CreateMessage::reference_message((channel_id, msg_id))`                         |
| Send media         | `send_media_group(chat_id, Vec<InputMedia>)`                   | `send_message(channel_id, CreateMessage::add_file(CreateAttachment))`            |
| Edit message       | `edit_message_text(chat_id, msg_id, text)`                     | `Message::edit(&ctx, EditMessage::new().content(text))`                          |
| Media per message  | 10 (media group), 50MB payload                                 | 10 attachments, 25MB per file (standard bot)                                     |
| Mentions           | implicit (in private chats)                                    | `msg.mentions_me(&ctx)`                                                          |
| Singleton access   | `TelegramBot::instance()` initialized synchronously in `run()` | `DiscordBot` needs `Arc<Http>` which only exists after `Client::builder().await` |
| Per-message author | `msg.chat.id` (resolves to user in DMs)                        | `msg.author.id` (always user) + `msg.guild_id.is_none()` for DM detection        |

---

## Design Decisions (with rationale)

### D1: Dispatch model — mention-gated CLI parsing for both DMs and guilds (no slash commands)

**Decision:** Keep the existing dispatch logic verbatim:

- DMs → always parse as command
- Guilds → require explicit `@Bot` mention, then parse as command

**Rationale:**

- The current code at `cmd/discord/bot/mod.rs:175` already implements this correctly:
  ```rust
  if msg.guild_id.is_some() && !msg.mentions_me(&ctx).await.is_ok_and(|x| x) {
      return;
  }
  ```
- `mentions_me` is true **only** for explicit `@Bot` mentions (not `@everyone`/`@here`/`@role`).
- A slash-command version was considered and rejected: it adds meaningful complexity (slash command registration, `InteractionCreate` handler, deferred-response lifecycle, dual StatusMessage abstraction) without clear UX benefit given the mention-based flow is already working.
- CLI parsing via clap is shared between the two paths — single source of truth for the command grammar.

**Trade-off acknowledged:** Future Discord UX improvement could add slash commands on top; the design below (singleton DiscordBot, StatusMessage backed by message IDs only) is slash-command-agnostic and a future `/download` slash command could be added without restructuring.

### D2: Where the shared `Downloadable` trait lives — `cmd/_common/`

**Decision:** Move `cmd/telegram/common/downloadable/` → `cmd/_common/downloadable/` (verbatim). Telegram's `common/mod.rs` re-exports it so existing imports keep working.

**Rationale:**

- The `cmd/_common/` directory already exists empty — clearly the intended shared location.
- The `Downloadable` trait + `FileReference` impl are **not** Telegram-specific; they're used to download via HTTP or iroh blob tickets, which both bots need.
- Only one external import site (`cmd/telegram/bot/handlers/message/mod.rs:37`) uses the current path; a re-export keeps that working without churn.
- Alternative (lift to `app-helpers` crate) was rejected — larger blast radius (touches shared crate), and this is bot-specific orchestration.

### D3: URL extraction — `linkify` crate

**Decision:** Add `linkify = "0.10"` to `bins/downloader-bot/Cargo.toml` and use it for URL extraction from message content.

**Rationale:**

- Telegram uses teloxide's entity parser (Telegram-native). Discord has no equivalent in serenity.
- Alternatives considered:
  - **Naive whitespace split + `Url::parse`** — zero deps but mishandles URLs adjacent to punctuation, URLs in parens, trailing slashes, etc.
  - **`linkify`** — well-maintained, zero-dep (no transitive deps), handles edge cases. Already idiomatic for Rust.
- Discord attachment URLs (`msg.attachments[i].url`) come pre-parsed as `String`; we just `Url::parse` them and concatenate.

### D4: Discord upload cap — configurable, default 25MB

**Decision:** Add `max_payload_size: Size` field to `DiscordBotConfig`, default `"25MB"`.

**Rationale:**

- Standard Discord bots can upload files up to 25MB each (some boosted servers allow up to 500MB). The bot's effective limit depends on where it's used.
- Telegram has the same config field (`telegram_bot.rs:57-58`) with default `"50MB"`. Mirror the pattern for symmetry.
- `Size` (via `size` crate) parses human-readable values like `"25MB"`, `"100MiB"`, etc.
- Exposed via env `DOWNLOADER_HUB_DISCORD_MAX_PAYLOAD_SIZE` and CLI `--discord-max-payload-size` for runtime override.

### D5: `DiscordBot` singleton pattern (mirrors `TelegramBot`)

**Decision:** Create a `DiscordBot` singleton holding `Arc<serenity::http::Http>` + `Arc<DiscordBotConfig>`. Initialize from `client.http.clone()` **before** `client.start_autosharded()`.

**Rationale:**

- Background tasks (`process_work_request`, `watch_work_requests`) need to make Discord API calls (edit status messages, upload attachments) without owning a `Context`.
- `serenity::Client` exposes `pub http: Arc<Http>` — safe to clone pre-start.
- The `Http` client is the only piece needed for sending messages / editing; cache is not required for these operations.
- This matches Telegram's `TelegramBot::instance()` pattern, minimizing conceptual divergence.

**Trade-off:** The serenity `Context` provides more than just `Http` (cache, shard info, etc.). For this bot's needs, `Http`-only is sufficient — no cache queries are required for sending/editing messages.

### D6: StatusMessage design — simple struct, `author_id` included

**Decision:**

```rust
struct StatusMessage {
    channel_id: ChannelId,
    msg_id: MessageId,             // original user message (reply target)
    author_id: UserId,             // enables owner-check from background tasks
    #[serde(default)]
    status_msg_id: Option<MessageId>, // bot's status message, created on first update
}
```

**Rationale:**

- Telegram's StatusMessage has `chat_id`, `msg_id`, `reply_msg_id`. We mirror this with the addition of `author_id`.
- `author_id` is required because: in `process_work_request` (background task reading metadata), we need to check whether the request came from the bot's owner (for save-dir behavior). Discord's `MessageId` doesn't carry author info, unlike Telegram's `ChatId` which resolves to a user in DMs.
- Serialized to JSON, stored as `metadata["status_message"]` in the work request. Deserialized by `watch_work_requests` to recover state.
- No interaction-token abstraction needed (D1: no slash commands).

### D7: StatusMessage methods use direct API calls (not broadcaster)

**Decision:** StatusMessage's `update_message`/`delete_message`/etc. call Discord API directly via `DiscordBot::bot()`. The `MessageBroadcaster` is reserved for the final outbound upload messages (where the existing retry loop in `cache_ready` provides resilience).

**Rationale:**

- Telegram's StatusMessage awaits each API call — subsequent code knows whether the status update succeeded.
- The broadcaster is fire-and-forget (`broadcast::Sender`); using it for status updates would lose ordering and error visibility.
- Status updates are not on the hot path (one edit per state transition); direct calls are fine.
- For the **final** file-upload messages, we use the broadcaster so the existing retry-on-`Io`/`ExceededLimit` loop applies — important for large uploads that may hit rate limits.

### D8: `process_work_request` location — separate `handlers/work_request.rs`

**Decision:** Put `watch_work_requests` and `process_work_request` in a new `handlers/work_request.rs` file (mirror Telegram's `handlers/message.rs`).

**Rationale:**

- The function is ~250 lines; inlining in `bot/mod.rs` would make that file unwieldy.
- Telegram splits handlers into `command/` and `message/`; we mirror with `handlers/message.rs` (intake-side) and `handlers/work_request.rs` (processing-side).
- Cleaner module boundaries, easier to navigate.

### D9: Owner save-dir check via `author_id` in metadata

**Decision:** When `process_work_request` runs, it checks `status_message.author_id == DiscordBot::owner_id()` AND `DiscordBot::owner_download_dir().is_some()`. If both true, copy downloaded files to that directory.

**Rationale:**

- Mirrors Telegram's behavior (`telegram/.../message/mod.rs:305-322`) where the owner's downloads are also saved locally.
- Telegram's check is `status_message.chat_id().as_user() == Some(owner_id)` — checking if the chat is a DM with the owner.
- For Discord, `author_id` is the user-spread equivalent.

### D10: Cleanup of `error_formatter.rs`

**Decision:** Delete the file and the commented-out `// mod error_formatter;` declaration.

**Rationale:**

- The module is currently unused (`// mod error_formatter;`).
- It uses unstable `u32::is_multiple_of` (line 176) — would not compile on stable Rust.
- Current behavior renders clap errors as plain text inside a code fence; that's acceptable.
- Resurrecting it would require fixing the unstable-feature use and wiring it into clap's `ErrorFormatter` machinery. Not worth it for marginal formatting improvements.
- The file is dead weight; deleting keeps the codebase clean.

### D11: Free-form URL intake as fallback when CLI parse fails

**Decision:** When `BotCommand::parse` fails AND `urls_in_message(msg)` returns non-empty, treat the message as a download request (Telegram's `handle_message` behavior).

**Rationale:**

- Telegram's dispatcher (`telegram/.../mod.rs:159-162`) does exactly this: command parse fails → `handle_message` runs.
- Discord users in DMs (or after mentioning the bot in a guild) should be able to just paste URLs without `BotCommand::DownloadAndFix { urls: [...] }` CLI syntax.
- This is more forgiving UX and matches the Telegram behavior user-expectation-wise.
- The existing `BotCommand::DownloadAndFix` variant stays for explicit invocation (some users prefer the explicit form).

---

## Implementation Plan

### Phase 1 — Shared `Downloadable` → `cmd/_common/`

1. **New** `bins/downloader-bot/src/cmd/_common/mod.rs`:
   ```rust
   pub mod downloadable;
   ```
2. **Move** `cmd/telegram/common/downloadable/{mod.rs, impls/}` → `cmd/_common/downloadable/{mod.rs, impls/}` (verbatim — no code changes).
3. **Edit** `cmd/telegram/common/mod.rs`: replace `pub mod downloadable;` with `pub use crate::cmd::_common::downloadable;`.
4. **Edit** `cmd/mod.rs`: add `pub mod _common;` (alphabetically before `discord`).

**Verification:** `just dev-build downloader-bot` should compile with no semantic changes (Telegram imports unchanged due to re-export).

### Phase 2 — Config: add `max_payload_size`

**Edit** `crates/app-config/src/conditional/discord_bot.rs`:

- Add `use size::Size;`
- Add field at the end of `DiscordBotConfig`:
  ```rust
  /// The maximum size of a payload to send to the Discord API.
  ///
  /// If not set, the default value will be used.
  #[arg(long = "discord-max-payload-size", value_name = "MAX_PAYLOAD_SIZE", env = "DOWNLOADER_HUB_DISCORD_MAX_PAYLOAD_SIZE", value_hint = ValueHint::Other, default_value = "25MB")]
  pub max_payload_size: Size,
  ```

**Verification:** `just dev-build downloader-bot` (and `downloader-central`, `downloader-worker` — they share the crate transitively).

### Phase 3 — `DiscordBot` singleton

**New** `bins/downloader-bot/src/cmd/discord/bot/discord_bot.rs`:

```rust
use std::sync::{Arc, OnceLock};
use app_config::common::Size;
use app_config::conditional::discord_bot::DiscordBotConfig;
use serenity::http::Http;
use serenity::model::id::UserId;

pub struct DiscordBot {
    http: Arc<Http>,
    config: Arc<DiscordBotConfig>,
}

static DISCORD_BOT: OnceLock<DiscordBot> = OnceLock::new();

impl DiscordBot {
    pub fn init(http: Arc<Http>, config: Arc<DiscordBotConfig>) {
        _ = DISCORD_BOT.set(Self { http, config });
    }

    pub fn instance() -> &'static Self {
        DISCORD_BOT.get().expect("Discord bot not initialized")
    }

    pub fn bot() -> &'static Arc<Http> {
        &Self::instance().http
    }

    pub fn owner_id() -> Option<UserId> {
        Self::instance().config.owner_id.map(UserId::new)
    }

    pub fn owner_download_dir() -> Option<std::path::PathBuf> {
        Self::instance().config.owner_download_dir.clone()
    }

    pub fn max_payload_size() -> Size {
        Self::instance().config.max_payload_size
    }
}
```

**Edit** `cmd/discord/bot/mod.rs`: declare `pub mod discord_bot;`.

**Edit** `cmd/discord/mod.rs::run`: between `Client::builder(...).await?` and `client.start_autosharded()`:

```rust
DiscordBot::init(client.http.clone(), Arc::new(config.bot.clone()));
```

### Phase 4 — StatusMessage helper

**New** `bins/downloader-bot/src/cmd/discord/bot/helpers/mod.rs`:

```rust
pub mod status_message;
pub mod file_group;
```

**New** `bins/downloader-bot/src/cmd/discord/bot/helpers/status_message.rs`:

- Struct as specified in D6.
- Methods mirror `telegram/.../status_message.rs`:
  - `from_message(&Message) -> Self`
  - `chat_id() -> ChannelId`
  - `msg_replying_to_id() -> MessageId`
  - `status_msg_id() -> Option<MessageId>`
  - `author_id() -> UserId`
  - `send_sub_message(&self, text: &str) -> Option<Self>` — sends new message, returns new StatusMessage with `status_msg_id = Some(new_msg.id)`
  - `send_additional_message(&self, text: &str) -> Option<Message>` — sends a sibling message, returns the raw `Message`
  - `update_message(&mut self, text: &str)` — edits `status_msg_id` if set, else sends a new message and stores its ID. Retry on `UnknownMessage` (clear `status_msg_id` and resend).
  - `delete_message(&self)` — deletes `status_msg_id` if set.
  - `to_metadata() -> HashMap<String, String>` — `{"status_message": serde_json::to_string(self)}`
  - `from_metadata(&HashMap) -> Result<Self, serde_json::Error>` — inverse.
- All async API calls use `DiscordBot::bot()` (the `&Arc<Http>`).
- Errors are logged (warn) and swallowed, matching Telegram's pattern — processing continues regardless of status-update failures.

### Phase 5 — File-grouping helper

**New** `bins/downloader-bot/src/cmd/discord/bot/helpers/file_group.rs`:

```rust
pub async fn files_to_attachment_groups<TFiles, TFile>(
    files: TFiles,
    max_size_bytes: u64,
) -> (Vec<Vec<CreateAttachment>>, Vec<(PathBuf, String)>)
where
    TFiles: IntoIterator<Item = TFile> + Send,
    TFile: AsRef<Path>,
```

Logic:

- For each file in parallel (`FuturesUnordered`):
  - Get MIME via `app_helpers::file_type::infer_file_type`
  - Get size via `tokio::fs::metadata`
  - On error: collect into failed vec with reason.
- Walk the collected file infos:
  - If file size > `max_size_bytes`: collect into failed.
  - Else: build `CreateAttachment::path(path, filename)` and add to current chunk.
  - When chunk reaches 10 attachments: flush to result vec, start new chunk.
- Return `(groups, failed)`.

**Simplification vs Telegram's `file_group.rs`:**

- No `ChunkGroup` partitioning (Telegram has Document/Audio/Other groups because media groups must be homogeneous — Discord has no such constraint).
- No image dimension checks (Telegram rejects oversized images to avoid JPG conversion — Discord handles attachments uniformly).
- No GIF/PNG-as-document special-casing (Telegram-only quirk).

### Phase 6 — Message handler refactor

**New** `bins/downloader-bot/src/cmd/discord/bot/handlers/mod.rs`:

```rust
pub mod message;
pub mod work_request;
```

**New** `bins/downloader-bot/src/cmd/discord/bot/handlers/message.rs`:

URL extraction helper:

```rust
use linkify::{LinkFinder, LinkKind};
pub fn urls_in_message(msg: &Message) -> Vec<Url> {
    let mut urls: Vec<Url> = LinkFinder::new()
        .links(&msg.content)
        .filter(|l| matches!(l.kind(), LinkKind::Url))
        .filter_map(|l| Url::parse(l.as_str()).ok())
        .collect();
    urls.extend(msg.attachments.iter().filter_map(|a| Url::parse(&a.url).ok()));
    urls.sort();
    urls.dedup();
    urls
}
```

Main intake function:

```rust
pub async fn handle_download_request(msg: &Message, urls: Vec<Url>) -> Result<(), ...> {
    let mut status_message = StatusMessage::from_message(msg);

    if urls.is_empty() {
        status_message.update_message("Message doesn't contain any file or URL").await;
        return Ok(());
    }

    status_message.update_message("Processing message...").await;

    let mut added_some = false;
    for (i, url) in urls.into_iter().enumerate() {
        let mut url_status = status_message
            .send_sub_message(&format!("Processing URL: {}", url))
            .await
            .unwrap_or_else(|| status_message.clone());

        let max_bytes = DiscordBot::max_payload_size().bytes().cast_unsigned();
        let file_ref = FileReference::url(FileUrl::from(url).with_max_filesize(Some(max_bytes)));

        let resp = RpcClient::work_request_create(
            RequestInfo::DownloadAndFix(file_ref),
            url_status.to_metadata(),
            Some(format!("discord-{}-{}-{}", msg.channel_id, msg.id, i)),
        ).await;

        // ... handle RpcResponse::Data / Error / Err ...
        // ... parse WorkRequestCreateResponse, update status with ID ...
        // ... set added_some = true on success ...
    }

    if !added_some {
        status_message.update_message("Failed to add any requests to queue").await;
    } else {
        status_message.delete_message().await;
    }

    Ok(())
}
```

(Mirror `telegram/.../message/mod.rs:42-148` line-for-line in spirit.)

**Edit** `cmd/discord/bot/mod.rs`:

The `message` handler:

- Keep existing dispatch (mention check, mention-strip, CLI parse).
- `BotCommand::Ping` / `About` — unchanged.
- `BotCommand::DownloadAndFix { urls }` — call `handlers::message::handle_download_request(&msg, urls).await`.
- On `BotCommand::parse` error:
  - If `urls_in_message(&msg)` non-empty → `handlers::message::handle_download_request(&msg, urls).await` (free-form intake).
  - Else → send clap error via broadcaster (current behavior, minus `dbg!`).
- Remove placeholder `hello.txt` block.
- Remove `dbg!(&e)`.
- Remove `// MessageBroadcaster::get().send();` comment.

### Phase 7 — Work request processing

**New** `bins/downloader-bot/src/cmd/discord/bot/handlers/work_request.rs`:

```rust
pub async fn watch_work_requests() -> Result<(), anyhow::Error> {
    // (identical to current bot/mod.rs::watch_work_requests,
    //  but on each work request:)
    for req in work_requests.iter() {
        let status_message = match StatusMessage::from_metadata(&req.metadata) {
            Ok(x) => x,
            Err(e) => {
                error!(?e, "Failed to get status message");
                continue;
            }
        };
        tokio::task::spawn(process_work_request(req.clone(), status_message));
    }
}
```

`process_work_request` mirrors `telegram/.../message/mod.rs:152-399`:

1. Per-request `Semaphore` lock via static `LazyLock<Arc<Mutex<HashMap<Arc<str>, Arc<Semaphore>>>>>`.
2. Branch on `work_request.status`:
   - `Pending` → update status "Request is waiting for processing..." → return.
   - `Failed { reason }` → update status "Request failed: {reason}" → return.
   - No `progress_info()` → return.
   - `!progress.waiting_for_requester` → if `progress.message`, show it → return.
3. If `progress.files_data` is `None`/empty → mark complete via `RpcClient::work_request_complete`, delete status, return.
4. Parallel-download `progress.files_data` (`Semaphore::new(4)`, `TempFile::new_with_prefix("downloader-bot-dl-")`, `FileReference::download_into(tokio_file)`).
5. Owner save-dir: if `status_message.author_id() == DiscordBot::owner_id()` and `DiscordBot::owner_download_dir().is_some()` → `copy_files_to_save_dir`.
6. Chunk via `files_to_attachment_groups(downloaded_paths, DiscordBot::max_payload_size().bytes().cast_unsigned())`.
7. Collect all errors (failed downloads, failed chunks, `work_request.errors`) → one additional status message.
8. For each attachment group:
   - `DiscordBot::bot().send_message(channel_id, CreateMessage::new().add_file(att0).add_file(att1)...).reference_message((channel_id, msg_id))` via the **broadcaster** (for retry).
   - On broadcast failure (logged by the loop): continue.
9. Delete status message.
10. `RpcClient::work_request_complete(request_id)`; check `is_ok()`.

**Edit** `bot/mod.rs::cache_ready`: the existing `watch_work_requests()` call now dispatches to `handlers::work_request::watch_work_requests()`. The broadcaster-consumer loop stays as-is (it's used by intake and the final uploads).

**Remove** the now-duplicated `watch_work_requests` and `handle_broadcast` from `bot/mod.rs` if appropriate (or keep `handle_broadcast` since it's still called by the cache_ready loop).

### Phase 8 — Cleanup

- **Delete** `cmd/discord/bot/error_formatter.rs`.
- **Edit** `cmd/discord/bot/mod.rs`: remove `// mod error_formatter;` line.

### Phase 9 — Dependency + Verification

- **Edit** `bins/downloader-bot/Cargo.toml`: add `linkify = "0.10"`.
- Run `just fmt-dev` (per AGENTS.md — never raw `cargo fmt`/`clippy`).
- Run `just dev-build downloader-bot`.
- Manual smoke test via `mprocs` (`bot-discord` entry) with the dev token from `.env`:
  - DM the bot a YouTube URL → expect status message → expect uploaded media.
  - Mention `/ping` and `/about` in a guild → expect old behavior preserved.
  - Mention with explicit `/download_and_fix <url>` in a guild → expect download flow.
  - Send a message with a Discord attachment → expect it to be processed.

---

## File Manifest

| File                                                                       | Action                                                                                                                                                              |
| -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bins/downloader-bot/Cargo.toml`                                           | add `linkify = "0.10"`                                                                                                                                              |
| `bins/downloader-bot/src/cmd/mod.rs`                                       | add `pub mod _common;`                                                                                                                                              |
| `bins/downloader-bot/src/cmd/_common/mod.rs`                               | **new**                                                                                                                                                             |
| `bins/downloader-bot/src/cmd/_common/downloadable/mod.rs`                  | **moved** from telegram                                                                                                                                             |
| `bins/downloader-bot/src/cmd/_common/downloadable/impls/mod.rs`            | **moved** from telegram                                                                                                                                             |
| `bins/downloader-bot/src/cmd/_common/downloadable/impls/file_reference.rs` | **moved** from telegram                                                                                                                                             |
| `bins/downloader-bot/src/cmd/telegram/common/mod.rs`                       | `pub use crate::cmd::_common::downloadable;`                                                                                                                        |
| `crates/app-config/src/conditional/discord_bot.rs`                         | add `max_payload_size: Size` field                                                                                                                                  |
| `bins/downloader-bot/src/cmd/discord/mod.rs`                               | `DiscordBot::init(...)` before `start_autosharded`                                                                                                                  |
| `bins/downloader-bot/src/cmd/discord/bot/mod.rs`                           | real `DownloadAndFix` + free-form URL handling; remove placeholder + `dbg!`; drop `error_formatter`; declare new submodules; move `watch_work_requests` to handlers |
| `bins/downloader-bot/src/cmd/discord/bot/discord_bot.rs`                   | **new** — `DiscordBot` singleton                                                                                                                                    |
| `bins/downloader-bot/src/cmd/discord/bot/helpers/mod.rs`                   | **new**                                                                                                                                                             |
| `bins/downloader-bot/src/cmd/discord/bot/helpers/status_message.rs`        | **new**                                                                                                                                                             |
| `bins/downloader-bot/src/cmd/discord/bot/helpers/file_group.rs`            | **new**                                                                                                                                                             |
| `bins/downloader-bot/src/cmd/discord/bot/handlers/mod.rs`                  | **new**                                                                                                                                                             |
| `bins/downloader-bot/src/cmd/discord/bot/handlers/message.rs`              | **new** — `handle_download_request` + URL extraction                                                                                                                |
| `bins/downloader-bot/src/cmd/discord/bot/handlers/work_request.rs`         | **new** — `watch_work_requests` + `process_work_request`                                                                                                            |
| `bins/downloader-bot/src/cmd/discord/bot/error_formatter.rs`               | **delete**                                                                                                                                                          |

---

## Risks & Trade-offs

### Risk: Discord CDN attachment URLs expire (~24h)

**Mitigation:** The work request is processed promptly (worker starts as soon as central assigns it, typically within seconds). The bot's `process_work_request` is also fast (parallel downloads with `Semaphore::new(4)`). For longer worker queues, attachments may fail to download; this would surface as a work-request failure that the bot relays back to the user. Future improvement: download the attachment to iroh blobs on the bot side and pass a `FileReference::BlobTicket` to the worker (immune to CDN expiry).

### Risk: Status updates may race with `process_work_request`

**Mitigation:** Telegram uses a per-request `Semaphore` lock map to ensure only one `process_work_request` runs per request ID at a time. We mirror this exactly (`WORK_REQUESTS_PROCESSING_LOCKS` static).

### Risk: `DiscordBot::init` race

**Mitigation:** `init()` is called synchronously in `cmd::discord::run` before `client.start_autosharded()`. All background tasks that consume `DiscordBot::instance()` are spawned from `cache_ready`, which fires after the bot is fully connected — well after `init()` returns. The `OnceLock` provides a clean panic-on-double-init guard.

### Trade-off: StatusMessage errors are swallowed

The `update_message` / `delete_message` methods log errors but do not propagate them (matching Telegram). This means a transient Discord outage during status updates won't abort the download pipeline. Trade-off: the user may see stale status text. Acceptable for a download-status indicator; would not be acceptable for a financially-critical system.

### Trade-off: No slash commands

The current dispatch requires `@Bot` mention in guilds (verbose vs slash commands). This is intentional for parity with the existing pattern and avoids the slash-command lifecycle complexity. Slash commands could be layered on top later without restructuring (the `DiscordBot` singleton and message-based `StatusMessage` are slash-agnostic).

---

## Open Follow-ups (out of scope for this work)

- Implement slash commands as a UX improvement (`/download urls:... attachments:...`). Requires a separate `InteractionCreate` handler and a deferred-response-aware StatusMessage variant.
- Implement iroh-blob-ticket attachment handling for CDN-expiry-immune attachment uploads.
- Wire up `BotCommand::Help` / `BotCommand::Start` for parity with Telegram (currently Discord has only Ping/About/DownloadAndFix).
- Consider per-guild slash command registration for instant dev feedback (vs global propagation delay).
- Move `Downloadable` and friends further down (e.g., into `app-helpers` crate) if other binaries need them.

---

## Verification Steps for Future Agents Continuing This Work

1. `just dev-build downloader-bot` — must compile cleanly.
2. `just fmt-dev` — must pass (this runs rustfmt + clippy with the repo's pedantic+nursery config).
3. Check `bins/downloader-bot/src/cmd/discord/bot/error_formatter.rs` is gone.
4. Check `bins/downloader-bot/src/cmd/_common/downloadable/` exists and Telegram's `common/mod.rs` re-exports it.
5. Check `crates/app-config/src/conditional/discord_bot.rs` has `max_payload_size` with `default_value = "25MB"`.
6. Check `linkify` is in `bins/downloader-bot/Cargo.toml`.
7. Manual smoke test via `mprocs` (bot-discord entry):
   - DM a URL → download completes.
   - DM a Discord attachment → download completes.
   - Mention `@bot /ping` in a guild → "Pong!".
   - Mention `@bot <url>` in a guild → download completes.
