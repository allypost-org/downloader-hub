# Move Control-Plane RPC to `irpc` over QUIC

> **Status: Implemented (2026-07-01).** All phases complete; the workspace builds
> clean and the old HTTP/WS/JWT stack is deleted. Verification = the manual
> `mprocs` end-to-end run described in "Verification Steps" below.

## TL;DR

The control-plane RPC between `downloader-central`, `downloader-worker`, and
`downloader-bot` is today a hand-rolled mix of **axum HTTP** (`POST /api/rpc`),
**WebSocket** (`/api/v1/ws`), and a **vestigial signed-gossip auth path**, all
carrying one giant versioned `Message` enum with **no request/response
correlation**. `iroh` itself is used only for blob transfer. This plan replaces
that control plane with **`irpc`** (riding the *existing* iroh `Endpoint` via
the `irpc-iroh` transport), moves pub/sub broadcasts onto `iroh-gossip`, and
makes **API keys** (validated against Convex) the stable identity so workers and
bots can migrate between hosts arbitrarily. The result: one QUIC transport,
typed request/response with per-call backchannels, and the deletion of a large
surface of mechanical dispatch/codec/auth boilerplate.

---

## Background & Context

### Process architecture (from root `AGENTS.md`)

Four binaries share state via **Convex** and (notionally) communicate via
**iroh**:

- `downloader-central` — axum HTTP coordination server + iroh node.
- `downloader-worker` — performs downloads/processing; iroh blob provider.
- `downloader-bot` — Telegram + Discord bots; iroh blob provider.
- `downloader-cli` — standalone local tool, **not** part of the mesh.

### How the RPC actually works today (the starting point)

Despite the docs saying "communicate via iroh", the control RPC does **not** run
over iroh. There are three concurrent transport mechanisms, all carrying the
same `app_peer_comms::Message`:

**(a) HTTP RPC** — `POST /api/rpc`
(`bins/downloader-central/.../worker_api/routes/mod.rs:31` → `routes/rpc.rs:9`
`post_rpc`). Body = JSON `Message`; auth = JWT bearer (`ValidAuth`). Dispatched
via `rpc_handler/mod.rs:5` `handle_rpc`. Returns `Resp::{Data, Error}`. The
**bot** is the client (`bins/downloader-bot/src/peering/rpc/mod.rs:47`
`RpcClient::request`).

**(b) WebSocket** — `/api/v1/ws`
(`routes/v1/mod.rs:16` → `root.rs:109` `any_ws` → `event_handler/mod.rs:18`
`handle_socket`). Text = JSON, Binary = postcard
(`event_handler/handlers/socket_message/mod.rs:20-37`). The **worker** is the
client (`bins/downloader-worker/src/cmd/work/app/mod.rs:61`,
`tokio_tungstenite::connect_async`). Related watch routes: `/watch/work-requests`,
`/watch/work-requests/{id}`, `/events/mine`.

**(c) Signed gossip (largely vestigial)** —
`bins/downloader-central/.../components/peers/mod.rs:24` `run` subscribes to the
gossip topic, verifies `SignedMessage` (`message/mod.rs:92`
`verify_and_decode`), and routes into the **same** `handle_rpc`. In practice it
only meaningfully processes `Authorize` for unauthenticated peers; the real auth
happens over HTTP.

### What iroh is actually used for

- **`iroh-blobs`** — file transfer. Worker adds finished files with expiring tags
  (`__expiring-<rfc3339>`), mints `IrohBlobTicket`, embeds it in
  `WorkRequestMoveToWaitingForRequester` as `FileReference::BlobTicket`.
  Requester downloads via `PeeringEndpoint::download_ticket_into`
  (`crates/app-peer-comms/src/lib.rs:392`).
- **`iroh-gossip`** — subscribed by central only; `broadcast_encoded_message` has
  **zero callers** (dead code).
- **`iroh-mdns-address-lookup`** — LAN discovery.
- **`iroh-tickets`** — join tickets.

There is **no** `open_bi`/`accept_bi` stream usage anywhere in the workspace —
iroh's QUIC streams are not used for control RPC today.

### The shared protocol type

`crates/app-peer-comms/src/message/`:

- `mod.rs:14` — `enum Message { V1(V1Message) }` (V1-only versioned wrapper).
- `v1/mod.rs:12` — `enum V1Message { Worker(..), Central(..), Bot(..) }`
  (discriminated by **sender role**).
- `v1/worker/mod.rs:10` — `WorkerMessage`: `Authorize | WorkRequestTake |
  WorkRequestFree | WorkRequestUpdateStatusMessage | WorkRequestAddErrors |
  WorkRequestMoveToWaitingForRequester | WorkRequestFail`. Also defines
  `CommunicationType::{Json, Postcard}` (`:50`) — the hand-rolled codec selector.
- `v1/bot/mod.rs:9` — `BotMessage`: `Authorize | WorkRequestMake |
  WorkRequestGetMineInProgress | WorkRequestAddErrors | WorkRequestComplete`.
- `v1/central/mod.rs:18` — `CentralMessage`: 13 variants — a mix of responses
  (`WorkRequestsTakeResponse`, `WorkRequestCreateResponse`,
  `WorkRequestFinishResponse`) and `..Result` siblings
  (`WorkRequestUpdateStatusMessageResult`, `WorkRequestAddErrorsResult`,
  `WorkRequestMoveToWaitingForRequesterResult`, `WorkRequestFailResult`), plus
  broadcasts (`WorkRequest`, `WorkRequests`, `WorkRequestFreed`,
  `WorkRequestFailed`) and auth (`AcceptAuthentication`/`RejectAuthentication`).
- `v1/common/mod.rs:7` — `RequestId = Arc<str>` (the **business** work-request
  ID, not an RPC call ID).
- `v1/common/authentication/mod.rs:9` — `Authentication::{ApiKey(Arc<str>),
  JwtPair, RefreshToken}` — **API keys are already a concept**.
- `mod.rs:75` — `SignedMessage { from, data, signature, auth, created_at }` —
  Ed25519-signed gossip envelope.

### Where the pain is

1. **No request/response correlation.** `RequestId` is the business ID, not a
   call ID. The worker fires `WorkRequestTake` into a tokio `broadcast` channel
   and trusts that *some* later `WorkRequestsTakeResponse` with the same
   `request_id` arrives. There is no `oneshot`/pending-future map anywhere.
2. **Massive enum-match boilerplate.** `event_handler/mod.rs:78-132` (worker)
   and `rpc_handler/v1/{bot,worker}/mod.rs` (central) are hundreds of lines of
   mechanical `match` arms, many just `debug!(?res, ..)`. Every new RPC method
   touches: request enum, response enum, result struct, central handler arm,
   worker/bot handler arm, a broadcaster helper.
3. **Two parallel transport stacks** (HTTP `/api/rpc` for the bot, WS
   `/api/v1/ws` for the worker) both funnelling into the same `handle_rpc`, each
   with its own framing, auth extraction, and content negotiation.
4. **Three layers of broadcaster indirection** on central: RPC handler →
   `Broadcaster::send_to_audiences` (tokio broadcast) → per-WS
   `handle_broadcasts` task → `SocketSender` → WS frame.
5. **Stringly-typed errors.** `RpcError` collapses DB errors into
   `Generic(String)`; wire `*ErrMessage` enums carry only a static `&str`.
6. **Awkward typed-response unwrapping.** Callers must pattern-match a 4-level
   `Message::V1(V1Message::Central(CentralMessage::XResponse(..)))` or fall back
   to `"Invalid response"`.
7. **Vestigial signed-gossip + `SignedMessage` stack** that duplicates the HTTP
   auth flow.
8. **Duplicated retry/boot logic** in worker (`work/mod.rs:100-145`) and bot
   (`peering/mod.rs:19-86`).

---

## Why `irpc` (and `irpc-iroh`)

[`irpc`](https://docs.rs/irpc/0.17.0/irpc/) is a minimal RPC framework by the
iroh team (n0 computer). Its documentation explicitly describes replacing *"an
mpsc channel with a giant message enum where each enum case contains mpsc or
oneshot backchannels"* — which is exactly what this codebase has grown into.

### Goals / non-goals (from the crate docs)

- **Goal:** lightweight enough to also be used for in-process async boundaries
  with zero overhead; abstract lightly over remote vs local.
- **Interaction patterns:** rpc (1:1), server-streaming, client-streaming,
  bidi-streaming — via per-call `oneshot`/`mpsc` backchannels.
- **Serialization:** `postcard` (length-prefixed varints).
- **Non-goals:** cross-language interop (Rust↔Rust only); versioning (you do it
  yourself); making remote calls *look* like local async fn; runtime agnosticism
  (tokio only).

These non-goals are all acceptable here: we are Rust-only, we control all
deployments (coordinated upgrades), we already use tokio, and we already use
postcard.

### Transport: `irpc-iroh` rides the existing iroh `Endpoint`

`irpc` core is transport-abstracted via the `RemoteConnection` trait (implemented
for `noq::Connection`, i.e. QUIC-by-socket-address). For **iroh** (QUIC by
NodeId) there is a companion crate
[`irpc-iroh`](https://docs.rs/irpc-iroh/0.17.0/irpc_iroh/), which provides:

- `IrohProtocol` — an `iroh::protocol::ProtocolHandler`. Registered on iroh's
  `Router` (alongside the existing `iroh_gossip::ALPN` and
  `iroh_blobs::ALPN` handlers) via `router.spawn(...)`.
- `irpc_iroh::client(endpoint, node_id, alpn)` — returns an irpc `Client` that
  dials central over QUIC.
- `irpc_iroh::listen` / `handle_connection` — server side.

**Version compatibility (verified):** `irpc-iroh 0.17.0` depends on `iroh ^1` and
`irpc ^0.17.0`. The project uses `iroh = "1.0"`
(`crates/app-peer-comms/Cargo.toml:23`), and `irpc 0.17.0` is already in
`Cargo.lock` as a transitive dep of `iroh-blobs`/`iroh-gossip`. So adding
`irpc = "0.17"` + `irpc-iroh = "0.17"` directly is a drop-in — **one QUIC stack**
(the existing `PeeringEndpoint`), no second endpoint, reusing the existing
relay/derp/mdns setup.

### Why not just keep HTTP?

HTTP request/response over axum is genuinely simple for 1:1 calls, but: (a) the
worker path is already not HTTP (it's WS), so we already pay the "two transports"
cost; (b) the broadcast/fan-out model has no clean HTTP expression (hence the
SSE endpoints and `BroadcastAudience` machinery); (c) consolidating onto the iroh
endpoint we already operate makes the "communicate via iroh" claim in
`AGENTS.md` true for the first time; (d) UDP/QUIC is confirmed available between
all hosts.

---

## Design Decisions (with rationale)

### D1: `irpc` + `irpc-iroh` over the existing `PeeringEndpoint`

**Decision:** Add `irpc = "0.17"` and `irpc-iroh = "0.17"` to `app-peer-comms`.
Register an irpc `IrohProtocol` on the existing iroh `Router`. Clients use
`irpc_iroh::client(PeeringEndpoint::global().router.endpoint(), central_node_id,
RPC_ALPN)`.

**Rationale:**
- Reuses the single QUIC stack, the existing relay/derp/mdns/node-id setup.
- `irpc`'s `#[rpc_requests]` derive generates the dispatch boilerplate that
  today is hand-maintained across 4 files.
- Per-call `oneshot`/`mpsc` backchannels give real request/response correlation
  (eliminates pain point #1).
- Same authors as iroh, already a transitive dependency — low trust risk.

**Trade-off:** `irpc` is `0.x` (actively evolving). Mitigation: pin exact
versions (`=0.17.0` if needed); accept churn cost in exchange for deleting the
custom stack.

### D2: API-key auth, stored in Convex `downloader_hub_authed`

**Decision:** API keys are the **stable identity**. They live in the existing
Convex `downloader_hub_authed` table. Central validates a key via the existing
`Database::authed_get_info_by_token(api_key)` call
(`bins/downloader-central/.../routes/v1/auth/mod.rs:16`), which already returns
`AuthedInfoResponse::{Authorized(AuthedInfo{id, for_role, expires_at}),
NotAuthorized}`.

**Rationale:**
- Workers/bots migrate between hosts arbitrarily — the iroh NodeId changes on
  every redeploy/move, so node-id cannot be the identity. The API key is the
  stable, revocable, host-independent credential.
- `downloader_hub_authed` is already the source of truth for `/auth/token`; the
  `Authentication::ApiKey` variant already exists
  (`message/v1/common/authentication/mod.rs:10`). No new storage.
- Revocation is dynamic (a Convex mutation), shared across central instances,
  survives central redeploy.

**Trade-off:** One Convex lookup per auth event. Mitigated by D3 (one validation
per connection, not per call).

### D3: Connection-scoped `Auth { api_key }` session

**Decision:** Each irpc/QUIC connection begins with an `Auth { api_key }` request.
Central validates the key (D2), and binds `{api_key → AuthedInfo(role, expiry)}`
to **that connection's lifetime**. All subsequent calls on the connection require
a bound session; unauthenticated calls are rejected. No JWT is issued or used on
the irpc path.

**Rationale:**
- QUIC connections are long-lived; validating once per connection is cheap and
  natural. Mirrors how TLS/QUIC authenticate once at handshake.
- Eliminates the JWT issuance + refresh-loop machinery
  (`work/global.rs`, `peering/mod.rs:75`, `jwt/`) from the RPC path entirely.
- irpc has no "per-call header" concept, so connection-scoped state is the
  idiomatic place for auth context.
- The iroh NodeId from the QUIC handshake is still available as
  transport-level peer info (useful for logging/diagnostics), it just isn't the
  identity.

**Trade-off:** A revoked key would remain valid for up to the connection's
remaining lifetime. **Resolution (decided):** a dedicated background task on
central runs a Convex **live query** over `downloader_hub_authed` and, on any
revoke / expire / delete event, tears down the `Session`(s) bound to that key
(closing the affected irpc connection so the client must re-auth — which then
fails). This gives prompt revocation without a per-call DB read.

### D4: Tiny HTTPS bootstrap endpoint retained

**Decision:** Keep a single HTTPS endpoint on central (e.g. `POST /api/v1/bootstrap`,
replacing `/join-ticket` + `/auth/token` + `/auth/refresh`) that:

1. Receives the API key.
2. Validates it via `authed_get_info_by_token` (defense in depth + fail-fast: an
   invalid/expired key gets a clear HTTPS error before any QUIC dial).
3. Returns central's iroh **NodeId + relay URL + RPC ALPN** so the caller can
   dial the QUIC endpoint.

The caller then dials central over QUIC and completes the D3 `Auth` handshake.

**Rationale:**
- Chosen by the user over env-baked discovery. Dynamic: ops never need to
  re-bake `.env` when central's NodeId changes (e.g. central itself is
  redeployed/migrated).
- Two-layer validation (HTTPS + irpc) is intentional: the HTTPS layer is the
  public entry point; the irpc layer is authoritative because a node could in
  principle be dialed directly.
- Keeps a minimal axum surface alive (see D6).

**Trade-off:** axum + TLS must remain up for new peers to join. Acceptable —
it's one tiny route.

### D5: Big-bang cutover (no parallel-run period)

**Decision:** The deployed system flips from HTTP/WS to irpc in one release.
There is no period where both old and new serve traffic simultaneously.

**Rationale (user-chosen):**
- The four binaries are deployed together (single `docker buildx bake` rolls all
  three images via Watchtower). A parallel-run period would require version
  skew handling that adds complexity for little benefit at this scale.
- Avoids carrying dead code in production.

**Development interpretation:** Big-bang is about the *deployed runtime*, not
the commit history. Development still proceeds in **compiling, reviewable
steps** (add irpc scaffolding → wire calls → delete old stack), each commit
building cleanly. The "flip" happens when the branch merges and images roll.

**Trade-off / risk:** A regression has no in-place fallback. Mitigation:
thorough manual verification (see Verification Steps) before merge; keep the
old branch recoverable via git.

### D6: axum survives for bootstrap + `/metrics` + `/health`

**Decision:** Post-migration central keeps a **minimal** axum router:
- `POST /api/v1/bootstrap` (D4).
- `GET /metrics` (iroh endpoint metrics — already exists at
  `routes/v1/root.rs:97`; standard Prometheus HTTP scrape).
- `GET /health` (liveness).

Everything else (`/api/rpc`, `/api/v1/ws`, `/watch/*`, `/events/mine`,
`/join-ticket`, `/auth/token`, `/auth/refresh`) is deleted.

**Rationale:**
- Chosen by the user (D4 implies axum stays for bootstrap). `/metrics` stays
  HTTP because Prometheus scrapes HTTP — moving it to QUIC would break standard
  observability tooling. `/health` is a standard container-liveness probe.

### D7: Broadcasts move to `iroh-gossip`

**Decision:** The pub/sub fan-out currently implemented via
`Broadcaster::send_to_audiences` + `BroadcastAudience` + the SSE `/watch/*` and
`/events/mine` endpoints moves to **iroh-gossip**. Central publishes
`CentralBroadcast` messages on the existing topic; worker/bot subscribe.

**Rationale:**
- Chosen by the user. irpc has no broadcast primitive (it's request/response
  with per-call channels), so pub/sub needs a separate mechanism.
- `iroh-gossip` is already a dependency and already wired into the
  `PeeringEndpoint::Router` (`iroh_gossip::ALPN`). It is currently vestigial
  (`broadcast_encoded_message` has 0 callers) — this revives it for its actual
  purpose.
- The `SignedMessage` envelope is redundant once we move to gossip: iroh-gossip
  already authenticates the sender by NodeId at the transport layer. So
  `SignedMessage`/`sign_and_encode`/`verify_and_decode` are deleted; the gossip
  payload is the plain postcard-encoded `CentralBroadcast`.

**Mapping of current → new:**
- `CentralMessage::WorkRequest` / `WorkRequests` (initial-state snapshot to new
  subscribers) → `CentralBroadcast::WorkRequest(..)` / `WorkRequests(..)`.
- `WorkRequestFreed` / `WorkRequestFailed` → matching broadcast variants.
- `/watch/work-requests` and `/events/mine` SSE endpoints → **deleted**. Bot's
  `work_request_watch_mine_in_progress`
  (`bins/downloader-bot/src/peering/rpc/work_request/mod.rs:34`) becomes a
  gossip subscription (filtered to "mine").

**Trade-off:** Gossip has message-size limits (large `WorkRequests` snapshots
may need chunking — see Risks). SSE gave free backpressure; gossip requires the
subscriber to keep up or drop.

### D8: Protocol shape — per-direction irpc enums; drop the `Message::V1` wrapper

**Decision:** Replace the `Message`/`V1Message`/`WorkerMessage`/`BotMessage`/
`CentralMessage` tree with:

1. **`CentralProtocol`** — one `#[rpc_requests]` enum describing all
   request/response calls central serves (central is the irpc server). Each
   variant declares its response channel (`tx`) and optional request stream
   (`rx`).
2. **`CentralBroadcast`** — a plain serde enum for gossip payloads (no
   backchannel; fire-and-forget pub/sub).

See "Target Architecture" below for the concrete shape.

**Rationale:**
- irpc's model is one service enum per server. Central is the only server, so
  one `CentralProtocol` enum.
- The sender-role discrimination (`Worker` vs `Bot`) moves into the
  connection-scoped `AuthedInfo.for_role` (D2/D3): central knows which role is
  calling because the connection authenticated as that role. Some methods are
  role-gated (e.g. `WorkRequestTake` is worker-only).
- irpc explicitly punts on versioning; we drop the `V1` wrapper and rely on
  coordinated binary upgrades (we control all deployments). A
  `PROTOCOL_VERSION: u32` constant is kept for future use (logged on connect;
  mismatch becomes a future gate).

### D9: Serialization = postcard only; delete `CommunicationType`

**Decision:** Use postcard everywhere (irpc mandates it). Delete
`CommunicationType::{Json, Postcard}`, the `Accept`/`Content-Type` negotiation,
the `Negotiated`/`JsonOrAccept` extractors, and all the dual-codec match sites.

**Rationale:** irpc serializes with postcard and length-prefixes with postcard
varints. Supporting JSON on the wire was a debug convenience that added 4
hand-rolled negotiation sites. Debuggability is preserved via tracing logs and
postcard being human-decodable with tools.

### D10: Typed errors per call

**Decision:** Each irpc method's `tx` channel returns a typed result enum (e.g.
`TakeResult::{Ok(Box<WorkRequest>), Err(TakeResultErr)}`), where the error
variant is a structured serde type (not `String`). The current `*Result`/`
*ErrMessage` types in `message/v1/central/*_result/` are reused/adapted.

**Rationale:** Eliminates the stringly-typed `RpcError::Generic(String)` and
the `RpcResponse::Error(String)` wire form. Typed errors propagate cleanly
through irpc's channels.

### D11: `iroh-blobs` file transfer is untouched

**Decision:** The blob-transfer path (`PeeringEndpoint::download_ticket_into`,
expiring tags, `IrohBlobTicket` embedded in work-request progress) is orthogonal
to the control-plane RPC and is **not** changed by this migration.

**Rationale:** It already works well and already uses iroh correctly. The
control-plane migration is independent.

---

## Target Architecture

```
                         ┌─────────────────────────────┐
                         │       Convex (state)        │
                         │  downloader_hub_authed      │
                         │  downloader_hub_requests    │
                         └─────────────┬───────────────┘
                                       │ (DB)
              ┌────────────────────────┴─────────────────────────┐
              │            downloader-central                    │
              │  iroh Endpoint (existing) ── Router spawns:      │
              │   • IrohProtocol  (irpc control RPC)   [NEW]     │
              │   • iroh_blobs    (file transfer)     [unchanged]│
              │   • iroh_gossip   (pub/sub broadcasts)[revived]  │
              │  + minimal axum: /bootstrap /metrics /health     │
              └───┬───────────────────────────────┬──────────────┘
        irpc/QUIC │ (control RPC, API-key session) │ gossip (broadcasts)
                  │                               │
       ┌──────────┴──────────┐         ┌──────────┴──────────┐
       │ downloader-worker   │         │ downloader-bot      │
       │ irpc Client ───────►│         │ irpc Client ───────►│
       │ gossip subscriber   │         │ gossip subscriber   │
       │ blobs provider      │         │ blobs provider      │
       └─────────────────────┘         └─────────────────────┘

  Bootstrap (HTTPS, once on startup):
    worker/bot ──POST /api/v1/bootstrap {api_key}──► central
    central ──► { nodeId, relayUrl, rpcAlpn } (after Convex key check)
    worker/bot dials central over QUIC, sends irpc Auth { api_key }
```

### `CentralProtocol` (irpc request/response — central is the server)

```rust
use irpc::{channel::{mpsc, oneshot}, rpc_requests};
use serde::{Deserialize, Serialize};

pub type RequestId = Arc<str>;

/// Per-call auth is implicit: bound to the connection by the `Auth` call.
/// `Auth` itself returns the session info on success.
#[rpc_requests(message = CentralRequest)]
#[derive(Debug, Serialize, Deserialize)]
enum CentralProtocol {
    /// Authenticate the connection. MUST be the first call. Binds {role, expiry}.
    #[rpc(tx = oneshot::Sender<AuthResult>)]
    #[wrap(Auth)]
    Auth { api_key: Arc<str> },

    // --- worker role ---
    #[rpc(tx = oneshot::Sender<TakeResult>)]      #[wrap(WorkRequestTake)]
    WorkRequestTake { request_id: RequestId },
    #[rpc(tx = oneshot::Sender<FreeResult>)]      #[wrap(WorkRequestFree)]
    WorkRequestFree { request_id: RequestId },
    #[rpc(tx = oneshot::Sender<UpdateStatusResult>)] #[wrap(WorkRequestUpdateStatus)]
    WorkRequestUpdateStatus { request_id: RequestId, message: Arc<str> },
    #[rpc(tx = oneshot::Sender<AddErrorsResult>)] #[wrap(WorkRequestAddErrors)]
    WorkRequestAddErrors { request_id: RequestId, errors: Vec<String> },
    #[rpc(tx = oneshot::Sender<MoveResult>)]      #[wrap(WorkRequestMoveToWaiting)]
    WorkRequestMoveToWaiting { request_id: RequestId, files_data: Vec<FileReference> },
    #[rpc(tx = oneshot::Sender<FailResult>)]      #[wrap(WorkRequestFail)]
    WorkRequestFail { request_id: RequestId, reason: Arc<str> },

    // --- bot role ---
    #[rpc(tx = oneshot::Sender<CreateResult>)]    #[wrap(WorkRequestMake)]
    WorkRequestMake { info: RequestInfo, metadata: HashMap<String, String>, idempotency_key: Option<String> },
    #[rpc(tx = oneshot::Sender<CompleteResult>)]  #[wrap(WorkRequestComplete)]
    WorkRequestComplete { request_id: RequestId },
    /// Server-streaming: snapshot of the bot's current "mine in progress", then
    /// live updates as Convex notifies central. Replaces /watch/work-requests SSE.
    #[rpc(tx = mpsc::Sender<WorkRequest>)]        #[wrap(WorkRequestGetMineInProgress)]
    WorkRequestGetMineInProgress,
}
```

### `CentralBroadcast` (gossip payload — pub/sub)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
enum CentralBroadcast {
    /// To a specific requester (audience filtering moves into the payload).
    WorkRequest { for_role: Role, request: Box<WorkRequest> },
    WorkRequests { for_role: Role, requests: Arc<[WorkRequest]> },
    WorkRequestFreed { request_id: RequestId },
    WorkRequestFailed { request_id: RequestId, reason: FailResultStatus },
}
```

Note: today's `BroadcastAudience::{Socket, Authed, Endpoint}` filtering moves
into an explicit `for_role` (or a `for_requester`) field on the payload, since
gossip is topic-wide and subscribers filter locally.

---

## Implementation Plan (big-bang; each step compiles)

> Conventions: use `just fmt-dev` for all lint/fix (never raw `cargo fmt`/`clippy`).
> Build-check per step with `just dev-build <package>`. No `cargo test` (the repo
> has no test suite — see root `AGENTS.md`).

### Phase 0 — Dependencies & protocol types (no behavior change)

1. **`crates/app-peer-comms/Cargo.toml`**: add `irpc = "0.17"`, `irpc-iroh = "0.17"`
   (features: `derive`). Confirm `Cargo.lock` resolves `irpc-iroh 0.17.0`
   (pulls `iroh ^1`, compatible).
2. **New** `crates/app-peer-comms/src/rpc/mod.rs`: define `CentralProtocol`,
   `CentralRequest`, `CentralBroadcast`, and the per-call result types. Reuse
   the existing result types from `message/v1/central/*_result/` where shapes
   match.
3. **Define** `RPC_ALPN: &[u8] = b"downloader-hub/rpc/1"`.
4. **Edit** `crates/app-peer-comms/src/lib.rs`: re-export `pub mod rpc;`.
5. `just dev-build app-peer-comms` — must compile (old `Message` tree still
   present, untouched).

### Phase 1 — Auth model (Convex lookup + connection session)

6. **New** `crates/app-peer-comms/src/rpc/session.rs`: a `Session` type holding
   the bound `AuthedInfo { id, for_role, expires_at }`. The irpc handler obtains
   the `Session` for the current connection (via a connection-keyed
   `DashMap<NodeId, Session>` or irpc's connection context) and rejects calls
   when absent/expired.
7. **Central**: implement the `Auth` arm — call
   `Database::global().authed_get_info_by_token(api_key)` (same call as
   `routes/v1/auth/mod.rs:16`), bind the result to the connection, return
   `AuthResult::{Ok(AuthedInfo), Err(AuthError)}`.
8. **Connection-expiry task**: when `AuthedInfo.expires_at` elapses, close the
   irpc connection (forces re-auth). Mirrors today's JWT-expiry WS close
   behavior.

### Phase 2 — irpc server (central) + clients (worker, bot)

9. **Central**: register `IrohProtocol::new(CentralProtocol, handler)` on the
   existing `PeeringEndpoint::global().router` (alongside the gossip/blobs
   protocols). Implement the handler as an actor over a
   `tokio::mpsc::Receiver<CentralRequest>` (the irpc-recommended shape — see
   crate example).
10. **Migrate each call** from the old dispatch into the irpc handler, reusing
    the existing Convex mutation calls (`Database::global().requests_*`). Order:
    `WorkRequestFree` → `AddErrors` → `UpdateStatus` → `MoveToWaiting` →
    `Fail` → `Take` → `Make` → `Complete` → `GetMineInProgress`.
11. **Worker**: replace the WS client
    (`bins/downloader-worker/src/cmd/work/app/{mod.rs,socket_sender.rs,
    broadcaster/mod.rs,event_handler/}`) with an irpc `Client` obtained via
    `irpc_iroh::client(endpoint, central_node_id, RPC_ALPN)`. Each former
    `send_work_request_*` helper becomes a typed `client.rpc(..).await?` call.
12. **Bot**: replace `peering/rpc/mod.rs` (HTTP `RpcClient`) and the WS watchers
    with the same irpc `Client`.

### Phase 3 — Broadcasts → iroh-gossip

13. **Central**: publish `CentralBroadcast` on the gossip topic via
    `PeeringEndpoint::broadcast_encoded_message` (now taking a `CentralBroadcast`
    instead of `Message`). Replace `Broadcaster::send_to_audiences` and its
    `BroadcastAudience` matching with direct gossip publishes.
14. **Worker/Bot**: `gossip_subscribe()` and decode `CentralBroadcast`; dispatch
    locally. The bot's "watch mine in progress" becomes a gossip subscription
    filtered by the bot's requester id (or stays as the streaming irpc call from
    Phase 2 step 10 — see Open Details).
15. **Delete** `routes/v1/{root::get_work_request_watch, events::any_mine}`,
    `event_handler/handlers/broadcast.rs`, and the central `broadcaster.rs`.

### Phase 4 — HTTPS bootstrap + delete the old stack

16. **Central**: add `POST /api/v1/bootstrap` (replaces `/join-ticket` +
    `/auth/token` + `/auth/refresh`). Validates API key via
    `authed_get_info_by_token`; returns `{ nodeId, relayUrl, rpcAlpn }`.
17. **Worker/Bot**: replace `init_peering_endpoint`'s `/api/v1/join-ticket`
    fetch + `JwtData::fetch_with_api_key` with a single `/bootstrap` call that
    returns the iroh node to dial; then perform the irpc `Auth` handshake.
18. **Delete**:
    - `routes/rpc.rs`, `routes/v1/root.rs::any_ws` (`/api/v1/ws`),
      `routes/v1/auth/`, `event_handler/handlers/socket_message`, both
      `socket_sender.rs` copies.
    - `crates/app-peer-comms/src/message/` (entire tree: `Message`,
      `V1Message`, `WorkerMessage`, `BotMessage`, `CentralMessage`,
      `SignedMessage`, `CommunicationType`).
    - `cmd/central/auth/` (JWT issuance), `cmd/central/rpc_handler/`.
    - The three `broadcaster` stacks, `RpcError`/`RpcResponse`,
      `JsonOrAccept`/`Negotiated`, `CommunicationType` codec sites.
19. **Reduce** central's axum router to `/api/v1/bootstrap`, `/metrics`,
    `/health`.

### Phase 5 — Cleanup & docs

20. Deduplicate worker/bot boot+retry logic into `app-peer-comms`.
21. Update `AGENTS.md`: root "Process architecture" (now genuinely iroh-based),
    `app-peer-comms` (rpc module + ALPN + session), and all three binaries'
    startup contracts.
22. Update `mprocs.yaml` if any dev flags change.
23. `just fmt-dev` over the whole workspace.

---

## File Manifest (target)

| Area | Action |
| --- | --- |
| `crates/app-peer-comms/Cargo.toml` | add `irpc`, `irpc-iroh` |
| `crates/app-peer-comms/src/rpc/{mod,session}.rs` | **new** — protocol enums + session |
| `crates/app-peer-comms/src/lib.rs` | re-export `rpc`; add `RPC_ALPN`; register `IrohProtocol` |
| `crates/app-peer-comms/src/message/` | **delete entirely** |
| `crates/app-peer-comms/src/jwt/` | **delete** (no JWT on RPC path) |
| `bins/downloader-central/.../components/worker_api/routes/rpc.rs` | **delete** |
| `bins/downloader-central/.../routes/v1/{root.rs,auth/,events.rs}` | **delete** (WS + auth + SSE) |
| `bins/downloader-central/.../routes/v1/bootstrap.rs` | **new** (HTTPS bootstrap) |
| `bins/downloader-central/.../rpc_handler/` | **delete** (replaced by irpc actor) |
| `bins/downloader-central/.../auth/` | **delete** (JWT issuance) |
| `bins/downloader-central/.../broadcaster.rs` | **delete** (→ gossip) |
| `bins/downloader-central/.../components/peers/mod.rs` | rewrite (gossip publish of `CentralBroadcast`) |
| `bins/downloader-worker/src/cmd/work/app/{mod,socket_sender,broadcaster,event_handler}` | **delete/rewrite** → irpc `Client` |
| `bins/downloader-worker/src/cmd/work/global.rs` | drop JWT; hold irpc `Client` + `Session` |
| `bins/downloader-bot/src/peering/rpc/` | **rewrite** → irpc `Client` |
| `bins/downloader-bot/src/peering/{mod,jwt}` | drop JWT refresh loop |

---

## Risks & Trade-offs

### Risk: `irpc` / `irpc-iroh` are `0.x`
Pin to `=0.17.0`. Accept potential upstream churn in exchange for deleting the
custom stack. Both crates are by the iroh team and used internally by
`iroh-blobs`.

### Risk: Big-bang with no test suite (D5)
No parallel-run fallback. Mitigation: exhaustive manual verification (below);
keep the pre-migration branch recoverable via git; roll images via Watchtower
only after smoke-testing dev.

### Risk: Gossip message-size limits
`WorkRequests` snapshots can be large; iroh-gossip fragments but has caps.
Mitigation: chunk initial-state snapshots in Phase 3 if needed; cap payload
size and log+drop oversized.

### Risk: Connection session outlives a revoked key (D3)
A revoked API key stays valid until the connection drops or `expires_at`
elapses. Mitigation: enforce `expires_at`-driven connection close; for
immediate revocation, maintain a denylist checked on each call (cheap Convex
read or an in-memory revocation set seeded from the outbox).

### Risk: UDP/QUIC reachability in production
Confirmed available between all hosts. QUIC needs UDP pass-through; verify the
deploy environment exposes UDP (and that iroh's relay/derp fallback is
acceptable for NAT'd peers).

### Risk: Bot/worker can't reach central if `/bootstrap` is down
The HTTPS bootstrap is a hard dependency for new connections (D4). Mitigation:
the bootstrap route is tiny and shares central's lifetime; central already
needs to be up to serve anything.

### Trade-off: Dropping JSON wire format (D9)
Loses ad-hoc `curl` debuggability. Mitigation: tracing logs; postcard is
decodable with tooling; keep a `--dump-protocol` debug flag if useful.

### Trade-off: No per-call auth (D3)
Auth is per-connection, so a misbehaving in-process caller could in principle
issue any role's call once authed. Mitigation: role-gate methods in the handler
(`WorkRequestTake` requires `role == Worker`, etc.) — cheap and explicit.

---

## Open Implementation Details (resolved)

All five flagged details are decided:

1. **Bootstrap endpoint authentication** → **API key required.** `/bootstrap`
   validates via `authed_get_info_by_token` (fail-fast) before returning
   `{ nodeId, relayUrl, rpcAlpn }`.
2. **"Mine in progress" delivery** → **streaming irpc call.**
   `WorkRequestGetMineInProgress` is a server-streaming call (snapshot + live
   Convex-driven updates over its `mpsc::Sender<WorkRequest>`). Gossip is used
   only for fan-out broadcasts (new work, frees, failures), not for the bot's
   own-request stream.
3. **Immediate revocation** → **Convex live-query task.** A background task
   watches `downloader_hub_authed` for revoke/expire/delete and tears down the
   bound sessions (see D3 trade-off resolution).
4. **ALPN string** → `b"downloader-hub/rpc/1"`.
5. **iroh-tickets dev path** → **kept.** Direct ticket join (LAN/mdns/dev, via
   existing `--peer-comms-*` flags) coexists with the HTTPS bootstrap
   (production).

---

## Verification Steps

Per root `AGENTS.md`, there is no test suite. Verification is manual end-to-end
via `mprocs` (5 entries: `db`, `central`, `worker`, `bot-telegram`,
`bot-discord`):

1. `just fmt-dev` — must pass (rustfmt + clippy pedantic+nursery, `unwrap_used`
   on warn).
2. `just dev-build downloader-central` / `-worker` / `-bot` — all compile.
3. Boot the mesh via `mprocs`:
   - Convex dev server (`just db-dev`) is reachable.
   - Central prints its iroh NodeId; worker/bot successfully call `/bootstrap`
     and establish an irpc session (visible in tracing).
4. **Telegram path**: DM the bot a URL → work request created (irpc) → taken by
   worker (irpc `WorkRequestTake`) → download+fix → blob transfer → bot
   downloads via `download_ticket_into` → uploads → `WorkRequestComplete` (irpc).
5. **Discord path**: same, via a guild mention and a DM.
6. **Broadcast**: confirm the bot receives work-request status updates via
   gossip (not SSE).
7. **Auth failure**: use an invalid API key → `/bootstrap` rejects; a direct
   QUIC dial without `Auth` → every call rejected with `Unauthenticated`.
8. **Revocation**: revoke the key in Convex → at `expires_at` the connection
   closes and re-auth fails.
9. **Ops**: `GET /metrics` and `GET /health` respond on central.

A regression with no fallback (D5) means: do not merge until 4–8 pass in dev.
