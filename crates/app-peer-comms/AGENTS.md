# app-peer-comms

iroh wrapper. Provides the `PeeringEndpoint` singleton that `downloader-central`, `downloader-worker`, and `downloader-bot` use to discover, connect to, and exchange data with each other. See root `AGENTS.md` for the process architecture and toolchain.

## Layout

- `src/lib.rs` — `PeeringEndpoint` (router + gossip + blobs + peer list) and `PeeringEndpointBuilder`. Global singleton via the `GlobalConfig` derive.
- `src/jwt/` — JWT pair (access + refresh) signing/verification. `targeted` variant scopes tokens to bot/worker.
- `src/message/` — signed gossip messages: `SignedMessage::sign_and_encode` / `verify_and_decode`. Versioned (`v1/`).
- `src/ticket/` — join tickets. `targeted::TargetedTicket` is the parsed form used at peer join; `TicketTarget::{Bot,Worker}` scopes it.
- `src/helpers/` — small utilities.

## Conventions

- Trace logs target `PeeringEndpoint::trace_span_name()` (the literal `"peering"`) — keep this stable; env log filters reference it.
- Direct peer connections are cached in a `moka` future cache (TTI 2 min, cap 30). Don't bypass it with ad-hoc `connect()` calls.
- Expiring blob tags are prefixed `__expiring-<rfc3339>`; `downloader-worker` periodically GCs them via `delete_expired_tags`.

## Builder

`PeeringEndpoint::builder(common_cfg, topic_id)` is the only public constructor. Chain `.with_peers(...)`, `.with_main_node(...)`, `.with_refresh_url(...)`, `.with_refresh_token(...)`, then `.await build()`.
