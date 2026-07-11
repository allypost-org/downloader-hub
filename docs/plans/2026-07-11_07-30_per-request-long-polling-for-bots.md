# Per-request bot delivery watches and fenced `delivering` leases

## Goal

Replace the bot-side full-snapshot subscription
(`WorkRequestGetMineInProgress` → `WorkRequestSnapshot` → signature deduplication
→ `WorkRequestLockMap` / `WorkRequestGuard`) with one supervised task per request.
Each task watches only its request, relays worker progress, claims delivery through
an atomic database mutation, and completes the request after attempting delivery.

This is a **persistent request-scoped server stream**, not literal long polling.
It replaces one full-list Convex subscription with bounded, requester-authorized
per-request subscriptions. The feature is considered safe only with the delivery
lease, fencing token, cancellation behavior, and capacity bounds described below.

## Decisions

### User decisions

1. Use a request-scoped persistent stream and a `delivering` database state.
2. A delivery lease is fixed at **10 minutes**. Normal files are expected to
   finish downloading and uploading inside that bound; there is no lease renewal.
3. When restarting on a request that is already delivering, keep a recovery
   watcher open until the matching lease expires and the request returns to
   `inProgress(waitingForRequester=true)`. Do not support unrestricted immediate
   force-claiming.
4. Use one platform trait which owns both status-message operations and platform
   delivery behavior.
5. Preserve current partial-delivery semantics: report individual download/upload
   failures to the user, then complete the overall request after all files have
   been attempted.
6. Preserve both audit identities: `done.by` continues to mean the worker that
   produced the result; new done rows also record optional `deliveredBy` for the
   bot that delivered it.
7. Enforce at most **64 request watches per bot credential** and **512 globally**
   at central.
8. Deploy this change stop-the-world. Do not add an RPC v2 ALPN or dual protocol
   server. New positional wire variants are still append-only, and the old
   snapshot RPC remains in this first change so no existing discriminants shift.

### Non-negotiable correctness contract

- Delivery is **at least once**, not exactly once. A bot can successfully upload
  a file and crash before recording completion, so a later attempt can send it
  again.
- A delivery attempt is fenced by a random `deliveryAttemptId`. Only the current
  attempt may complete, release, or clean up the delivery lease. A stale task
  cannot complete or revert a newer attempt.
- A bot may watch only a request owned by its authenticated bot credential.
  Unauthorized and nonexistent requests produce the same non-disclosing terminal
  stream event.
- Passive worker-progress observation has no bot-side deadline. The 10-minute
  deadline begins only after a bot has claimed a delivery attempt.

## Current problems being removed

The existing `requests:getMineInProgress` Convex watch re-emits the complete set
of pending, in-progress, and recent-failed requests whenever any row changes.
Discord tries to suppress that fan-out with a lossy `status_signature`; Telegram
spawns processing work on every snapshot. Both then depend on the in-memory
`WorkRequestLockMap` for delivery exclusion. The mechanism is noisy, does not
survive restarts, and has materially different retry behavior between platforms.

The new design avoids list snapshots and makes Convex, rather than a process-local
semaphore, the authority for delivery ownership.

## Lifecycle and state model

```text
pending
  -> inProgress(workerBy, CleanupId, ...)
  -> inProgress(waitingForRequester=true, workerBy, filesData, CleanupId)
  -> delivering(
       workerBy, workerSince, claimedBy, deliveryAttemptId,
       filesData, CleanupId
     )
  -> done(by=workerBy, deliveredBy=claimedBy)

matching delivery lease expiry
  -> inProgress(waitingForRequester=true, workerBy, filesData, CleanupId)
```

`CleanupId` is always the active timer for the current state. Transitioning out of
a state cancels its old cleanup job. The delivery cleanup schedules a normal
`takeCleanup` after restoring the waiting state; `takeCleanup` itself is one-shot
and does not reschedule.

### Fenced delivery lease

A bot that sees `waitingForRequester=true` calls `ackDelivery`:

1. The mutation verifies `row.requester === requesterId` and verifies that the
   status is `inProgress` with `waitingForRequester=true`.
2. It creates a cryptographically random `deliveryAttemptId`.
3. It schedules `deliveryCleanup(requestId, deliveryAttemptId)` for 10 minutes.
4. It replaces the worker's waiting state with `delivering`, retaining the worker
   identity and file data, and cancels the previous waiting-state cleanup.
5. It returns the attempt ID and authoritatively decoded file references.

`finishDelivery`, `releaseDelivery`, and `deliveryCleanup` all require the same
attempt ID. They must return without changing state if the row is no longer
`delivering` or its ID differs. This prevents a delayed old task or old scheduled
function from changing a later delivery attempt.

The bot gives the delivery operation a timeout slightly below the 10-minute lease
(for example, 9 minutes 30 seconds) so it can release the matching lease before
the scheduled cleanup normally fires. If the process is killed or cannot release,
`deliveryCleanup` is the fallback. A released or expired attempt is retried by
its still-running recovery watcher; the prior attempt may have partially uploaded
files, so the at-least-once contract remains explicit.

## Database changes

### 1. Schema

**File:** `crates/app-database/convex/schema.ts`

Add `requestDelivering` to the `requests.status` union. Keep existing `by` on
`requestDone` for backwards-compatible worker attribution, and add the optional
new audit field so old done rows remain valid.

```ts
export const requestDelivering = {
  Type: v.literal("delivering"),
  since: v.int64(),
  workerSince: v.int64(),
  workerBy: v.id(authedId),
  claimedBy: v.id(authedId),
  deliveryAttemptId: v.string(),
  filesData: v.optional(v.string()),
  CleanupId: v.id("_scheduled_functions"),
};

export const requestDone = {
  Type: v.literal("done"),
  at: v.int64(),
  by: v.id(authedId),
  deliveredBy: v.optional(v.id(authedId)),
};
```

Add `v.object(requestDelivering)` to the `status` union. The existing indexes
already index `status.Type`; no new database index is required.

### 2. Claim, finish, release, and cleanup mutations

**File:** `crates/app-database/convex/requests.ts`

Add `MAX_DELIVERING_IDLE_TIME_MS` for the 10-minute delivery lease. The bot,
not Convex, owns the slightly shorter operation timeout described below.

#### `ackDelivery`

Add a public mutation with `{ requestId, requesterId }`. Its return union is:

```ts
{ ok: true, code: "Claimed", deliveryAttemptId: string, filesData?: string }
{ ok: false, code: "RequestNotFound" | "RequestNotSubmittedByYou" |
    "NotWaitingForRequester" | "AlreadyDelivering" }
```

The `AlreadyDelivering` response is only advisory to a race loser; it does not
include another attempt's ID or files. On success, preserve all worker fields in
the new delivering status as described above. Generate the attempt ID inside the
mutation, schedule `internal.requests.deliveryCleanup` with it, patch the row,
and cancel the previous `inProgress.CleanupId`.

The Rust DB enum must use `#[serde(tag = "code")]`, matching `TakeResult`.

#### `finishDelivery`

Add a dedicated public mutation with `{ requestId, requesterId,
deliveryAttemptId }`. Do not widen the legacy `finish` mutation or
`WorkRequestComplete` RPC.

It must verify requester ownership, `status.Type === "delivering"`, and the
matching attempt ID. On success it patches:

```ts
status: {
  Type: "done",
  at: BigInt(Date.now()),
  by: row.status.workerBy,
  deliveredBy: row.status.claimedBy,
}
```

It cancels the matching delivery cleanup. Its result distinguishes not found,
not-owner, not-delivering, stale attempt, and success. A stale attempt is not an
error that permits a retry with the old ID.

#### `releaseDelivery`

Add a public mutation with `{ requestId, requesterId, deliveryAttemptId }`
for the bot's delivery timeout and controlled retry path.
It performs the same ownership and attempt-ID checks as `finishDelivery`, cancels
the delivery cleanup, schedules the normal `takeCleanup` with the current
`tries`, and restores:

```ts
{
  Type: "inProgress",
  since: row.status.workerSince,
  by: row.status.workerBy,
  filesData: row.status.filesData,
  waitingForRequester: true,
  CleanupId: cleanupId,
}
```

A bot whose release succeeds remains subscribed and may claim the newly emitted
waiting state using a fresh attempt ID. Apply bounded backoff between delivery
attempts to avoid tight retry loops.

#### `deliveryCleanup`

Add an internal mutation with `{ requestId, deliveryAttemptId }`. It must only
act when the row is still `delivering` with that exact attempt ID. It restores the
same waiting state as `releaseDelivery` and schedules `takeCleanup`. A stale
cleanup is a no-op. It must not copy `claimedBy` into the worker `by` field.

### 3. Existing request mutations and query surface

- Keep `fail` worker-only. Delivery errors are not reported through the worker
  failure RPC because the agreed policy completes with a user-facing partial
  delivery notice.
- Update `cancel` so a delivering row becomes failed and its delivery cleanup is
  cancelled. Update `remove` to cancel a delivery cleanup before deletion.
- Add `delivering` to `requestStatusType`, `requests:getByStatus`,
  `RequestStatusType`, and its `as_str()` method.
- Add `delivering` to `RequestStatusNamespace`, `requests:counts`, its return
  validator, the Rust `RequestCounts` mirror, and the aggregate count result.
- Update `getMineInProgress` by adding `requestDelivering.Type.value` to the
  existing `inStatuses` array used by `mergedStream`; retain the recent-failed
  behavior unchanged.
- Leave `retry` restricted to done/failed. Admins cancel a delivering request
  before retrying it.
- Update the legacy `src/entity/requests/mod.rs` mirror or remove it if it is
  confirmed unused; it must not continue describing a nonexistent
  `WaitingForRequester` variant.

### 4. Requester-scoped watch query

The current `requests:get` query is an unscoped `ctx.db.get(requestId)` and must
not back a bot-facing request watch.

Add `requests:getMineById` with `{ requestId, requesterId }`. It returns the row
only when its requester matches, otherwise a non-disclosing unavailable result.
Add `requests_watch_mine_by_id(requestId, requesterId)` and an initial
requester-scoped query method to the Rust database API. `WorkRequestWait` uses
these methods exclusively. The unscoped `requests:get` may remain for trusted
callers, but central's bot RPC must never invoke it.

## Rust DB and shared message types

### 1. `app-database` API mirror

**File:** `crates/app-database/src/api/requests/mod.rs`

- Add `RequestStatus::Delivering` with all JSON fields required by the Convex
  state: delivery `since`, `worker_since`, `worker_by`, `claimed_by`,
  `delivery_attempt_id`, and optional `files_data`.
- Extend `RequestStatus::Done` with optional `delivered_by`.
- Add tagged DB result enums and database methods for `requests:ackDelivery`,
  `requests:finishDelivery`, and `requests:releaseDelivery`.
- Decode `filesData` only at the database-to-wire conversion boundary; do not
  send serialized JSON blob tickets across the postcard RPC protocol.

### 2. `app-peer-comms` message types

**Files:**

- `crates/app-peer-comms/src/message/v1/central/work_request/request/status/mod.rs`
- new result modules below `message/v1/central/`

Append `Delivering` to the end of `WorkRequestStatus`; do not insert it before
`Done` or `Failed`. Postcard encodes enum discriminants positionally.

```rust
pub enum WorkRequestStatus {
    Pending,
    InProgress(ProgressInfo),
    Done { at: u64, by: String },
    Failed { at: u64, by: String, reason: String },
    Delivering { since: u64, claimed_by: String },
}
```

The wire delivering state intentionally omits `files_data`. A recovery watcher
waits for the subsequent waiting-state emission, then obtains typed file
references from a successful `ackDelivery` response. The database retains the
serialized files for that restoration.

Create explicit wire result types rather than reusing database enums:

```rust
pub enum WorkRequestAckResult {
    Claimed {
        delivery_attempt_id: Arc<str>,
        files: Arc<[FileReference]>,
    },
    NotWaitingForRequester,
    AlreadyDelivering,
    NotFound,
    Unauthorized,
    BackendError,
}

pub enum WorkRequestFinishDeliveryResult {
    Ok,
    NotDelivering,
    StaleAttempt,
    NotFound,
    Unauthorized,
    BackendError,
}

pub enum WorkRequestReleaseDeliveryResult {
    Released,
    NotDelivering,
    StaleAttempt,
    NotFound,
    Unauthorized,
    BackendError,
}
```

Map database results to these types in central. Map missing `filesData` to an
empty typed file list and decode nonempty `filesData` to `FileReference` only for
`Claimed`. If decoding fails after a claim, central must release that exact attempt
before returning `BackendError`; it must not leave an un-deliverable lease active.

`WorkRequestStatus::TryFrom` handles the database delivering variant. The existing
optional `deliveredBy` is database/admin audit data and need not alter the already
positional wire shape of `Done`.

## RPC protocol and central

### 1. Append new RPC requests

**Files:** `crates/app-peer-comms/src/rpc/request.rs` and `rpc/mod.rs`

Add all new `CentralProtocol` variants at the end of the enum. Do not remove or
reorder `WorkRequestGetMineInProgress` in this change, even though new bots no
longer call it. That preserves every existing positional discriminant.

Add request payloads for:

- `WorkRequestWait { request_id, watch_id }` — server stream of
  `WorkRequestWatchEvent`.
- `WorkRequestAck { request_id }`.
- `WorkRequestFinishDelivery { request_id, delivery_attempt_id }`.
- `WorkRequestReleaseDelivery { request_id, delivery_attempt_id }`.
- `WorkRequestListMineInProgress` — one-shot startup scan.

`watch_id` is generated by the bot per stream and permits explicit cancellation
if receiver-drop cancellation is not exposed by the irpc channel implementation.

Use an event envelope for request watches instead of silently closing streams:

```rust
pub enum WorkRequestWatchEvent {
    Request(WorkRequest),
    Unavailable,
    Overloaded,
    BackendError,
}
```

`Unavailable` covers both non-owner and nonexistent requests so an authenticated
bot cannot use IDs to discover another bot's requests.

### 2. Central dispatch and bounded watches

**File:** `bins/downloader-central/src/cmd/central/components/rpc/mod.rs`

For all new delivery/watch RPCs, require the bot role. Map non-bot callers to the
corresponding explicit `Unauthorized` result or terminal watch event.

Implement `spawn_watch_request` as follows:

1. Acquire a per-authed-ID permit (64) and global permit (512), held for the
   lifetime of the subscription. If either is unavailable, send `Overloaded` and
   close the stream.
2. Perform the initial requester-scoped lookup. If unavailable, send
   `Unavailable` and close.
3. Subscribe using `requests_watch_mine_by_id(requestId, authedId)`.
4. Forward valid converted requests as `WorkRequestWatchEvent::Request`.
5. Send `BackendError` and end the stream after a Convex watch error; do not warn
   and call `next()` again, which could spin on repeated errors.
6. Release both permits when the task exits.

The stream must terminate promptly when the client drops its receiver. Verify the
irpc sender's cancellation signal in a focused integration/manual check. If the
channel does not provide one, maintain the `watch_id` in central and add an
explicit stop-watch RPC invoked by a client stream wrapper on drop. Do not rely
on a later database emission to discover that the receiver disappeared.

### 3. Bot RPC client and reconnect coordination

**Files:** `bins/downloader-bot/src/peering/rpc/mod.rs` and `peering/mod.rs`

Add client methods for the new RPCs and remove new-call-site use of
`work_request_watch_mine_in_progress`.

Replace unconstrained per-task `reconnect()` calls with a process-wide reconnect
coordinator:

- One caller performs ticket bootstrap and reauthentication.
- Other request tasks and the heartbeat await the same attempt.
- Failed attempts use exponential backoff with jitter.
- Request streams are reopened only after the shared reconnect succeeds.

The startup scan must use the same coordinator and retry until it succeeds or the
platform is shutting down. A database/central failure must not be treated as an
empty successful scan.

## Bot processing model

### 1. Keyed task supervisor

**New/updated files:** `bins/downloader-bot/src/cmd/_common/request_processor.rs`
and `_common/mod.rs`

Use a process-local keyed task supervisor that owns at most one task for each
request ID. Both successful `work_request_create` and the startup scan request a
start through it. The supervisor removes the key only when the task exits.

This registry prevents duplicate local watchers from concurrent create responses,
startup recovery, and reconnect races. It is task ownership only; it is not the
delivery lock and does not replace the database `ackDelivery` mutation.

### 2. One platform trait

Replace the incomplete `DeliverStrategy` sketch with one platform-owned trait.
The trait owns its concrete status-message state and platform reply context, so
there is no nonexistent shared `StatusMessage` type.

It provides operations equivalent to:

- update the status message;
- send a supplemental error message;
- delete the status message;
- copy successfully downloaded files to the owner directory when applicable;
- group files to platform-sized attachment batches;
- send a batch with its platform-specific reply context.

Implement it for Discord and Telegram by preserving their current behavior:
Discord's unchanged-content edit suppression, Telegram's retry behavior when a
status message disappears, their existing owner-directory checks, batch builders,
and reply parameters. Both implementations must be reconstructible from the
stored request metadata for startup recovery.

### 3. `watch_and_process`

Each supervised task follows this state machine:

1. Open `WorkRequestWait` through the shared reconnect coordinator. Consume the
   receiver with a normal `loop`/`match`: `Ok(None)` is terminal, while a receive
   error closes the stream and enters the coordinated reconnect path.
2. On `Pending`, update the status message and continue.
3. On in-progress worker status without `waitingForRequester`, relay its progress
   message and continue. There is no overall task timeout here.
4. On in-progress status with `waitingForRequester=true`, call `WorkRequestAck`.
   - `Claimed` supplies the attempt ID and authoritative typed files. Begin one
     delivery operation under the shorter-than-lease timeout.
   - `AlreadyDelivering` from a normal race-loser task ends that task.
   - `NotWaitingForRequester` reopens/continues the watch to obtain current
     state.
   - transport or backend errors use bounded retry plus the reconnect
     coordinator; they must not wait forever for an emission that may never
     occur.
5. On `Delivering` before this task has claimed an attempt, distinguish origin:
   - a normal task that just lost an ack race exits;
   - a startup/recovery task remains subscribed. It displays recovery status and
     waits for the matching cleanup/release to emit the waiting state, then
     claims a fresh attempt.
6. On `Done`, delete the status message and exit. On `Failed`, display the
   reason and exit. On `Unavailable`, `Overloaded`, or `BackendError`, exit or
   reconnect according to whether the event is terminal or transport-backed.

Do not wrap the entire watch lifecycle in `WORK_REQUEST_TIMEOUT`. Apply the
operation timeout only after `Claimed`.

### 4. Delivery attempt behavior

`download_and_deliver` takes `request_id`, `delivery_attempt_id`, authoritative
files, and `&mut impl PlatformDelivery`; it must not recover files from a
pre-ack watch emission.

Retain the current bounded-concurrency download flow and await/collect the file
futures before grouping, copying, or uploading. Reuse the existing correct
`join_all`/`FuturesUnordered` style; do not pass an iterator of futures to file
operations.

For each individual download/upload failure, accumulate a user-visible notice.
After every file has been attempted, call `WorkRequestFinishDelivery` with the
attempt ID. Retry finish with the reconnect coordinator while the operation lease
remains valid. Do not call the worker `fail` RPC for these partial delivery
errors.

On operation timeout, call `WorkRequestReleaseDelivery` with the attempt ID,
then stay/reopen a recovery watch with bounded backoff. If the release races with
scheduled cleanup, a stale/not-delivering result is harmless; the watch obtains
the actual state before any new claim. Never finish with a stale ID.

## Startup and handler integration

### Platform message handlers

**Files:**

- `bins/downloader-bot/src/cmd/telegram/bot/handlers/message/mod.rs`
- `bins/downloader-bot/src/cmd/discord/bot/handlers/message.rs`

After a successful `work_request_create`, construct the platform delivery object
from the current message/status context and ask the keyed supervisor to start the
request task. Keep the create idempotency behavior unchanged.

### Startup recovery

**Files:**

- `bins/downloader-bot/src/cmd/telegram/mod.rs`
- `bins/downloader-bot/src/cmd/discord/bot/mod.rs`

After platform initialization, run a retried `WorkRequestListMineInProgress`
scan. Reconstruct the platform delivery object from request metadata and submit
each pending, in-progress, or delivering request to the keyed supervisor.

For a delivering row the task is specifically a recovery watcher: it remains
subscribed through lease expiry instead of returning immediately. This closes the
previous restart-recovery gap without unsafe force-claiming.

## Admin and aggregate propagation

`delivering` is an operator-visible active state. Update all of the following:

- `convex/lib/requestCounts.ts` namespace union;
- `requests:counts` validator, status list, and typed return object;
- Rust `RequestStatusType`, `RequestCounts`, and JSON mirror;
- admin HTTP `parse_status_type` and request-detail serialization;
- frontend `RequestStatusType`, `RequestCounts`, status badge, dashboard count
  cards, request tabs, and detail rendering.

The UI should label it “Delivering” and show the standard active-state controls.
Admin cancel must transition it to failed and cancel the delivery cleanup; remove
must cancel that cleanup before deletion. The optional `deliveredBy` field should
be shown in request detail when present.

## Legacy removal boundary

After all bots use request-scoped watches and a later intentional wire-breaking
deployment is approved:

- remove `WorkRequestGetMineInProgress` and its central handler;
- remove `WorkRequestSnapshot` and `WorkRequestSnapshotError` after confirming
  no remaining references;
- remove the old Discord/Telegram snapshot loops, `status_signature`,
  `WorkRequestGuard`, and `WorkRequestLockMap`.
- update the README's bot-delivery description after the snapshot RPC is removed.

Do not delete or reorder the old RPC variant in this first implementation. The
new path is unused by old code and can coexist until legacy removal is explicitly
scheduled.

## Implementation order

1. Add schema states/audit fields and requester-scoped Convex query.
2. Implement fenced claim, finish, release, and cleanup mutations; update cancel,
   remove, list, status filtering, counts, and aggregate types.
3. Mirror all database types/results in Rust and run `bun run check`.
4. Append wire status/RPC/event/result types without changing existing variant
   order; retain old snapshot protocol types.
5. Implement central authorization, capacity permits, watch cancellation, and
   result mapping.
6. Add the bot reconnect coordinator and typed client methods.
7. Implement the task supervisor, common platform trait, request watcher, and
   delivery-attempt logic.
8. Migrate Telegram and Discord create/startup paths, then remove only their old
   local processing paths once the new paths compile and work.
9. Update admin Rust/frontend status/count/detail handling.
10. Run formatting/lint through `just fmt-dev`, `bun run check` in
    `crates/app-database`, and relevant package builds. Use `mprocs` for manual
    end-to-end verification.

## Verification

There is no general test suite. Add focused Convex-level checks where practical,
then perform the following manual verification with `mprocs`:

1. Basic request lifecycle: create, worker progress, claimed delivery, files,
   successful completion, and correct worker/deliverer audit fields.
2. Multiple URLs and multiple simultaneous requests: one supervised task per ID,
   no duplicate local watcher or duplicate delivery.
3. Two concurrent bot instances using one credential: exactly one claim wins.
4. Stale finish: claim A, release/expire it, claim B, then send A's finish; B
   remains delivering and only B can complete.
5. Stale cleanup: simulate a delayed cleanup from A after B claims; B remains
   delivering.
6. Restart while worker is processing: startup scan opens a watch and receives
   the later waiting state.
7. Restart while delivering: recovery watcher remains alive, lease cleanup
   restores waiting state, and the same task claims a new attempt without a
   second restart.
8. Delivery operation timeout: attempt is released or cleaned up safely and the
   next attempt is fenced; partial platform delivery is acknowledged as possible.
9. Cancel/remove while delivering: both cancel the active cleanup and leave no
   later state mutation.
10. A bot cannot watch another bot's request, even with a known ID; it gets only
    `Unavailable` and no row data.
11. Capacity: the 65th watch for one bot and the 513th global watch receive
    `Overloaded`; permits are released when streams close.
12. Receiver cancellation: dropping a watch promptly ends its Convex watch and
    frees both permits without waiting for a row update.
13. Central failure: many request tasks trigger a single coordinated reconnect,
    then reopen streams after reauthentication.
14. Admin: counts, filters, badge, detail audit, cancel, and remove all handle
    delivering rows.
15. Stop all binaries before deployment; bring up central and bots together;
    confirm existing pre-change database rows decode and run through the new
    lifecycle without schema validation errors.
