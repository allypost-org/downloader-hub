# downloader-bot

Multi-platform bot (Telegram + Discord + others) selected via the first positional subcommand. See root `AGENTS.md` for toolchain/build commands.

## Layout

- `src/main.rs` → `cmd::run(CmdConfig)` dispatches on `CmdConfig::Telegram(cfg)` / `CmdConfig::Discord(cfg)`.
- `src/cmd/_config.rs` — shared subcommand enum (`CmdConfig`).
- `src/cmd/telegram/` — teloxide-based Telegram bot (`bot/`, `common/`, `config.rs`).
- `src/cmd/discord/` — serenity-based Discord bot (`bot/`, `broadcaster.rs`, `config.rs`).
- `src/peering/` — peer-comms integration. `init_peering_endpoint(...)` joins the iroh mesh using the bot API key.
- `src/peering/jwt/`, `src/peering/rpc/` — auth + RPC helpers (including `RpcBroadcaster::init()`).

## Startup contract (`async_main` in `src/main.rs`)

1. `Config::init_parsed()`.
2. `app_helpers::config::init(config.dependency_paths)` — wires yt-dlp/ffmpeg paths.
3. `peering::init_peering_endpoint(config.peer)`.
4. `RpcBroadcaster::init()` — outbound RPC channel.
5. Platform-specific `cmd::run(...)`.

The `bot-telegram` and `bot-discord` entries in `mprocs.yaml` are the canonical dev invocations.
