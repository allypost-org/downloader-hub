# downloader-bot

Multi-platform bot (Telegram + Discord + others) selected via the first positional subcommand. See root `AGENTS.md` for toolchain/build commands.

## Layout

- `src/main.rs` -> `cmd::run(CmdConfig, Arc<RpcClient>, Arc<ProgramPathConfig>)` dispatches on `CmdConfig::Telegram(cfg)` / `CmdConfig::Discord(cfg)`.
- `src/cmd/_config.rs` - shared subcommand enum (`CmdConfig`).
- `src/cmd/telegram/` - teloxide-based Telegram bot (`bot/`, `common/`, `config.rs`). `Arc<TelegramBot>` (owns the `ProgramPathConfig`) is injected into handlers via `Dispatcher::dependencies` (dptree), together with the `Arc<RpcClient>`.
- `src/cmd/discord/` - serenity-based Discord bot (`bot/`, `broadcaster.rs`, `config.rs`). `Arc<DiscordBot>` is stored in the serenity TypeMap under `DiscordBotKey`; the `MessageBroadcaster` and `Arc<RpcClient>` are owned by the `Handler` (via `HandlerConfig`).
- `src/peering/` - peer-comms integration. `init_peering_endpoint(...)` joins the iroh mesh (HTTPS bootstrap -> iroh ticket), connects an authenticated irpc session to central (`rpc::RpcClient::connect` -> `Auth` with the bot API key), spawns the heartbeat, and returns the `Arc<RpcClient>`. `reconnect.rs` holds `RpcClient::reconnect` - a per-client single-flight coordinator owned by the `RpcClient`.
- `src/peering/rpc/mod.rs` - irpc `RpcClient` (over `irpc-iroh`): `work_request_create`, plus the per-request delivery RPCs `work_request_wait` (server-streaming), `work_request_ack`, `work_request_finish_delivery`, `work_request_release_delivery`, and the one-shot `work_request_list_mine_in_progress` startup scan.
- `src/cmd/_common/request_processor.rs` - process-local keyed task supervisor (one task per request id), the `PlatformDelivery` trait (status-message + delivery ops), the `watch_and_process` state machine, and `download_and_deliver`. All take the `Arc<RpcClient>` (or `&RpcClient`) as a parameter.

## Startup contract (`async_main` in `src/main.rs`)

1. `Config::init_parsed()`.
2. `peering::init_peering_endpoint(config.peer, capabilities)` - fetches the iroh ticket from `/api/v1/join-ticket`, builds the `PeeringEndpoint`, then `RpcClient::connect` (the `Auth` irpc call); returns the `Arc<RpcClient>` (heartbeat already spawned).
3. Platform-specific `cmd::run(config.cmd, rpc, dependency_paths)` - each platform runs a one-shot `startup_scan` (recovering in-progress/delivering requests as supervised per-request watchers) and then starts per-request `WorkRequestWait` streams from the message handlers. Per-request watches own their own lifecycles; delivery is fenced by the database `delivering` lease (`ackDelivery`/`finishDelivery`/`releaseDelivery`).

The `bot-telegram` and `bot-discord` entries in `mprocs.yaml` are the canonical dev invocations.
