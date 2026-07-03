# downloader-bot

Multi-platform bot (Telegram + Discord + others) selected via the first positional subcommand. See root `AGENTS.md` for toolchain/build commands.

## Layout

- `src/main.rs` → `cmd::run(CmdConfig)` dispatches on `CmdConfig::Telegram(cfg)` / `CmdConfig::Discord(cfg)`.
- `src/cmd/_config.rs` — shared subcommand enum (`CmdConfig`).
- `src/cmd/telegram/` — teloxide-based Telegram bot (`bot/`, `common/`, `config.rs`).
- `src/cmd/discord/` — serenity-based Discord bot (`bot/`, `broadcaster.rs`, `config.rs`).
- `src/peering/` — peer-comms integration. `init_peering_endpoint(...)` joins the iroh mesh (HTTPS bootstrap → iroh ticket) and connects an authenticated irpc session to central (`rpc::RpcClient::init` → `Auth` with the bot API key).
- `src/peering/rpc/mod.rs` — irpc `RpcClient` (over `irpc-iroh`): `work_request_create`, `work_request_complete`, and `work_request_watch_mine_in_progress` (server-streaming).

## Startup contract (`async_main` in `src/main.rs`)

1. `Config::init_parsed()`.
2. `app_helpers::config::init(config.dependency_paths)` — wires yt-dlp/ffmpeg paths.
3. `peering::init_peering_endpoint(config.peer)` — fetches the iroh ticket from `/api/v1/join-ticket`, builds the `PeeringEndpoint`, then `RpcClient::init` (the `Auth` irpc call).
4. Platform-specific `cmd::run(...)` — each platform spawns a reconnecting `watch_work_requests` loop (consuming the irpc `WorkRequestGetMineInProgress` stream) and calls `RpcClient` for create/complete.

The `bot-telegram` and `bot-discord` entries in `mprocs.yaml` are the canonical dev invocations.
