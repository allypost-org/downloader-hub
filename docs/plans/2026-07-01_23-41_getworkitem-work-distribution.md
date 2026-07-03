# Worker work-distribution via blocking `getWorkItem` + per-worker refusal tracking

## TL;DR

Replace the gossip-push + irpc-poll work discovery with a blocking `getWorkItem`
RPC: a parked worker call that central resolves the moment work it can handle is
available. Central **pre-takes** the item on the worker's behalf, so there is no
take race. A worker that can't process an item **refuses** it (central frees it,
decrements `tries`, and records the worker in a new `refusedBy` field) — central
then never re-hands that item to that worker, but a *new* worker (new API key)
can still pick it up. Eliminates the poll loop, the gossip work-discovery, the
worker's `RECENTLY_HANDLED` cache, the `IS_PROCESSING` semaphore, and the
`WorkRequestTake` / `WorkRequestGetAvailable` protocol methods.

---

## Background

The previous design (see `2026-07-01_14-54_move-control-rpc-to-irpc.md`) had
workers discover work two ways: an iroh-gossip push (`CentralBroadcast::WorkRequests`)
and an irpc snapshot poll (`WorkRequestGetAvailable`, every 2 s). This worked but:

- Gossip delivery over QUIC silently drops (observed `noq_udp … NetworkUnreachable`),
  so the push path can't be relied on alone.
- The 2 s poll gives ≤2 s handoff latency and a Convex query per worker per tick.
- Workers race to `take` (atomic in Convex, so safe, but wasteful).
- A worker that freed capacity mid-stream had to wait for the next poll/nudge.

This plan replaces both with a single, deterministic, sub-second mechanism.

---

## Design

### Worker loop (sequential, one item at a time)

```text
loop {
    let item = RpcClient::get_work_item().await?;   // central pre-takes, then blocks until it has one this worker hasn't refused
    if worker_can_process(item) {                    // extractor match + enabled (moved out of handle_work_request)
        download_and_fix(item).await;                // → move-to-waiting / free / fail / update-status / add-errors (unchanged)
    } else {
        RpcClient::refuse_work_item(item.request_id).await?;  // central: free + tries-=1 + append this worker to refusedBy
    }
}
```

### Central `WorkDistributor` (one actor task)

State: a FIFO of parked workers (`{ reply: oneshot::Sender<WorkRequest>, authed_id }`)
and the latest Convex "available" set (each item carries `refusedBy`).

- **`getWorkItem(W)`**: if some item exists with `W ∉ item.refusedBy` →
  `requests:take(item, W)` and send the taken `WorkRequest` on the worker's
  `reply` immediately; else park `W`. Sub-second handoff, no polling.
- **Convex `getAllAvailable` emission**: replace the local `available` set and
  re-run matching — for each item in order, hand to the first parked waiter not
  in its `refusedBy`.
- **`refuseWorkItem(id)` from worker `W`**: `requests:refuse(id, W)`; the
  Convex `getAllAvailable` stream then re-emits (the item is pending again) and
  matching resumes — but `W` is now in the item's `refusedBy`, so `W` is skipped
  and the item can go to a *different* waiter.

### Why no central accept-loop restructuring

iroh runs `ProtocolHandler::accept` in its own task **per connection**, and the
worker's calls are sequential — it is *either* parked on `getWorkItem` *or*
making processing calls, never both. A blocking `getWorkItem` therefore stalls
only that one idle worker's connection (exactly when it has nothing else to
send). Other workers and the bot are unaffected.

### `refusedBy` semantics

- Stored on the request row as `authed_id[]`.
- Keyed by the **stable** authed id (the API key identity), so a worker that
  migrates hosts but keeps its API key is still treated as the same refuser.
- A genuinely-new worker (new API key) has refused nothing → can receive any
  pending item, including ones every existing worker refused. This is the
  intended "new worker joins that *can* process it" path.
- **Known limitation:** if an *existing* worker's capabilities change on the
  same API key (e.g. an extractor is later enabled), it will not re-evaluate
  items it previously refused. A trivial "clear refusals" admin mutation would
  fix this if it ever matters; not included here.

### Disconnect during handoff (edge case)

Central takes the item, then sends it on the worker's irpc channel. If the
worker is gone, `reply.send` fails → central frees the item (plain `free`). As a
backstop, the existing Convex `takeCleanup` scheduled function (10-min idle)
frees any item whose taker vanished. Refusal-decrement is **not** applied here
(the worker never refused — it disconnected); `tries` accumulation in this rare
path is acceptable and capped by `MAX_TRIES`/`takeCleanup`.

---

## Changes by area

1. **Convex schema** (`convex/schema.ts`): add `refusedBy: v.optional(v.array(v.id(authedId)))` to `requests`.
2. **Convex `requestDataReturn`** (`convex/requests.ts:80`): include `refusedBy` so every request query returns it.
3. **Convex `refuse` mutation** (`convex/requests.ts`, modeled on `free`): cancel the scheduled `takeCleanup`, set status `pending`, `tries = max(0, tries − 1)`, append the worker id to `refusedBy` (dedup), bump `lastModified`.
4. **app-database Rust**: mirror `refused_by` on the requests entity + `RequestInfoResponse` (default empty); add `Database::requests_refuse(request_id, worker_id)`.
5. **Protocol** (`app-peer-comms/src/rpc/`): add `GetWorkItem` (`oneshot::Sender<WorkRequest>`) and `RefuseWorkItem { request_id }` (`oneshot::Sender<RefuseResult>`). **Remove** `WorkRequestTake` and `WorkRequestGetAvailable`. (The processing-call variants — `WorkRequestFree` / `UpdateStatus` / `AddErrors` / `MoveToWaiting` / `Fail` / `Make` / `Complete` / `GetMineInProgress` — stay.)
6. **Central** (`components/rpc/`): the `WorkDistributor` actor + `GetWorkItem`/`RefuseWorkItem` handler arms; the database component's Convex stream feeds the distributor instead of gossip. Drop the heartbeat re-publish and the free/fail gossip broadcasts.
7. **Worker** (`cmd/work/`): replace the gossip listener + 2 s poll with the `getWorkItem` loop; move the capability check inline; **delete** `handle_work_request`, `RECENTLY_HANDLED`, `IS_PROCESSING`.
8. **Gossip cleanup**: with nothing publishing or consuming, remove `CentralBroadcast`, `broadcast_raw`/`decode_raw`, the `GOSSIP` sender global, and the worker's gossip subscription. The `PeeringEndpoint` keeps its gossip capability for future use.

---

## Verification

Manual `mprocs` end-to-end (no test suite):

1. Send several URLs to the bot in one message → each should be handed to a
   worker within sub-second of the previous finishing (no 2 s poll gap).
2. Restart the worker while requests are pending → it immediately parks and
   picks up work (no waiting for a gossip/poll cycle).
3. An item no current worker can process → it stays pending (refused by each);
   adding a worker with a new API key that *can* process it picks it up.
4. A worker whose connection drops mid-handoff → the item is freed and
   re-handed (or re-queued by `takeCleanup` if central itself missed the send).
