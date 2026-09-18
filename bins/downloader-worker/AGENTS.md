# downloader-worker

Fetches download jobs from central and processes them using `app-actions`. See root `AGENTS.md` for toolchain/build commands.

## Layout

- `src/cmd/work/rpc.rs` - irpc `RpcClient` (over `irpc-iroh`); authenticates with the API key on connect (the `Auth` call). Worker-side methods: `get_work_item` (blocks until central hands an item this worker hasn't refused; central pre-takes), `refuse_work_item`, and the processing calls (free/update-status/add-errors/move-to-waiting/fail).
- `src/cmd/work/app/mod.rs` - main loop, wrapped in `keep_running("Worker", ...)`. Connects the irpc session, then loops: `getWorkItem` -> `can_process`? `process` : `refuse` -> repeat. Sequential (one item at a time).
- `src/cmd/work/app/process.rs` - the download -> fix -> blob-stage pipeline (`process_work_request` + `download_and_fix`).
- `src/cmd/work/app/broadcaster/mod.rs` - thin fire-and-forget facade over the irpc `RpcClient`; keeps the `send_work_request_*` names so the pipeline is unchanged.
- `src/cmd/work/config.rs` - clap `WorkerConfig` (peer-comms ticket/api, dependency paths, endpoints, request config).
- `src/cmd/list.rs` - `list` subcommand: dumps available downloaders/fixers/actions/extractors.

## Startup contract (`async_run` in `src/cmd/work/mod.rs`)

1. `Arc<Actions>` is built from `WorkerConfig` (`ActionCtx` from endpoint/dependency-paths/request configs + the disabled-entries set) before peering; it is re-`clone`d into every `keep_running` retry.
2. `init_peering_endpoint(config.peer)` - either parses a pre-provided ticket from config, or fetches one from central at `/api/v1/join-ticket` (HTTPS bootstrap). Returns the `Arc<PeeringEndpoint>` + central's iroh address.
3. Spawns `TaskRunner::run()`, an expired-tag cleanup loop (10 s interval), and a dynamic-settings refresh loop (30 s + jitter).
4. `keep_running("Worker", ...)` runs `app::run`, which: connects the irpc session (`RpcClient::connect` -> `Auth`, stored via `WorkerLoop::set_rpc`), then loops on `getWorkItem`. `WorkerLoop` (an `Arc`-shared handle created in `async_run`) owns the RPC slot and the secrets/dynamic-settings reads. Central pre-takes and hands each item; if the worker can't process it (no matching extractor), it `refuse`s it (central frees, decrements `tries`, records the worker in the item's `refusedBy`).

No JWTs: auth is API-key-at-connect; the retry schedule resets after a fixed 5 min window.
