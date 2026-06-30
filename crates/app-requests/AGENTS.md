# app-requests

Thin `reqwest` wrapper preconfigured with rustls + webpki roots. Provides `Client::builder()`; everything else passes through to `reqwest`.

Used for outbound HTTP across the workspace. See root `AGENTS.md` for toolchain/build commands.
