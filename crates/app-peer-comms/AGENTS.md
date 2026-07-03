# app-peer-comms

iroh wrapper + the irpc control protocol. Provides the `PeeringEndpoint` singleton that `downloader-central`, `downloader-worker`, and `downloader-bot` use to discover, connect to, and exchange data with each other. See root `AGENTS.md` for the process architecture and toolchain.

## Layout

- `src/lib.rs` — `PeeringEndpoint` (iroh `Endpoint` + `Router` + gossip + blobs + peer list) and `PeeringEndpointBuilder`. Global singleton via the `GlobalConfig` derive. Re-exports `irpc`/`irpc_iroh` and key iroh types (`IrohConnection`, `IrohProtocolHandler`, `IrohGossipSender`, …).
- `src/rpc/` — the irpc control protocol: `CentralProtocol` (`#[rpc_requests]`-derived, the wire-level request/response definition), `CentralRequest` (generated message enum), `CentralBroadcast` (gossip payload), `request` payloads, `AuthResult`, and `RPC_ALPN = b"downloader-hub/rpc/1"`.
- `src/message/v1/` — shared payload types reused by the irpc protocol + the binaries: `common/{RequestId, file::FileReference, request_info::RequestInfo}` and `central/{work_request::WorkRequest, *_result}`. (The old `Message`/`V1Message`/`WorkerMessage`/`BotMessage`/`CentralMessage` enums and `SignedMessage` were removed with the irpc migration.)
- `src/ticket/` — join tickets. `targeted::TargetedTicket` is the parsed form used at peer join; `TicketTarget::{Bot,Worker}` scopes it.
- `src/helpers/` — small utilities.

## Conventions

- Trace logs target `PeeringEndpoint::trace_span_name()` (the literal `"peering"`) — keep this stable; env log filters reference it.
- Direct peer connections are cached in a `moka` future cache (TTI 2 min, cap 30). Don't bypass it with ad-hoc `connect()` calls.
- Expiring blob tags are prefixed `__expiring-<rfc3339>`; `downloader-worker` periodically GCs them via `delete_expired_tags`.
- Gossip payloads are raw postcard via `PeeringEndpoint::broadcast_raw` / `decode_raw` (no envelope — iroh-gossip authenticates the sender by node id).
- **irpc wire types are postcard, not JSON.** Any type that crosses the irpc/postcard boundary (everything in `src/rpc/` and `src/message/v1/`, plus gossip payloads) is serialized **positionally** — there are no field tags or names. JSON-oriented serde attributes (`rename_all`, `rename`) are harmless because postcard ignores names, but **never** use `skip_serializing_if`, `skip`, `skip_serializing`, or any attribute that changes the *number* of serialized fields: a struct whose `Option` field is omitted on encode will decode with `Hit the end of buffer, expected more data` on the other side (and `#[serde(default)]` does not rescue this — that only helps self-describing formats). Keep wire structs fully positional; always serialize every field.

## Builder

`PeeringEndpoint::builder(common_cfg, topic_id)` is the only public constructor. Chain `.with_peers(...)`, `.with_main_node(...)`, `.with_refresh_url(...)`, `.with_router_hook(|b| b.accept(alpn, handler))` (to register an extra ALPN such as central's irpc handler), then `.await build()`.
