# syntax=docker/dockerfile:1
#
# Unified Dockerfile for all downloader-hub Rust binaries.
# Build a specific image via `docker buildx bake` (see `docker-bake.hcl`),
# which shares the `chef`/`planner`/`deps` stages across all targets so the
# workspace dependency graph is compiled exactly once.
#
# Named stages:
#   chef         - Rust toolchain + cargo-chef (shared)
#   planner      - cargo chef prepare (shared)
#   deps         - cargo chef cook, whole workspace (shared)
#   builder      - cargo build --package ${BINARY_NAME} + upx (per binary)
#   ffmpeg       - static ffmpeg (shared, used by bot + worker)
#   runtime-base - common alpine setup (shared)
#   bot          - final runtime image for downloader-bot
#   central      - final runtime image for downloader-central
#   worker       - final runtime image for downloader-worker

ARG RUST_VERSION='1.96'
ARG RUST_TARGET='x86_64-unknown-linux-musl'

ARG DNS_SERVERS='9.9.9.9,8.8.8.8,1.1.1.1'

ARG UPX_VERSION='5.2.0'
ARG YT_DLP_VERSION='latest'

ARG FFMPEG_DOWNLOAD_URL='https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz'

ARG RUNNER_CONTAINER='alpine:latest'

##########
# Step 0 #
##########
##
## Setup base image with cargo-chef.
## Shared across all targets.
##
FROM rust:${RUST_VERSION}-alpine AS chef
# `clang` is needed to build the musl C deps of some crates.
# `curl` is needed by the `builder` stage (upx download) and `bash` by the
# entrypoint-generation scripts. They are installed here so they are available
# in every stage that derives from `chef` (deps, builder).
ARG RUST_TARGET
RUN update-ca-certificates \
  && apk add --no-cache \
  curl \
  bash \
  clang \
  && cargo install --locked cargo-chef \
  && rustup target add "${RUST_TARGET}"
WORKDIR /app


##########
# Step 1 #
##########
##
## Generate a build plan for the whole workspace's rust dependencies.
## Shared across all targets.
##
FROM chef AS planner
WORKDIR /chef
COPY . .
RUN cargo chef prepare \
  --recipe-path recipe.json


##########
# Step 2 #
##########
##
## Build the whole workspace's dependencies once.
## Shared across all targets (no `--package`/`--features` here so the layer
## hash is independent of the chosen binary).
##
FROM chef AS deps
ARG RUST_TARGET
COPY --from=planner /chef/recipe.json recipe.json
# NOTE: do NOT cache-mount `/app/target` here. cargo-chef writes the cooked
# dependencies into that directory so they persist in the layer and are
# inherited by the `builder` stage. A cache mount would shadow those files
# and defeat cargo-chef's cross-stage reuse.
# The cargo registry is likewise NOT cache-mounted: baking the crate sources
# into this layer lets the concurrent `builder` stages inherit them via
# copy-on-write, so they never race on each other's `.cargo-ok` files.
RUN cargo chef cook \
  --release \
  --target "${RUST_TARGET}" \
  --recipe-path recipe.json


##########
# Step 3 #
##########
##
## Build a single binary on top of the cooked dependencies, then compress it.
## Per-binary: diverges via ${BINARY_NAME}.
##
FROM deps AS builder
ARG RUST_TARGET
ARG BINARY_NAME
ARG UPX_VERSION
ENV APPLICATION_NAME=${BINARY_NAME}
# Install upx (pinned) before building so build + compress can share one RUN.
RUN cd "$(mktemp --directory)" \
  && curl -sL \
  "https://github.com/upx/upx/releases/download/v${UPX_VERSION}/upx-${UPX_VERSION}-amd64_linux.tar.xz" \
  | tar xvJ \
  && cd "upx-${UPX_VERSION}-amd64_linux" \
  && mv upx /usr/bin \
  && cd .. \
  && rm -rf "$(pwd)" \
  && echo "Installed upx ${UPX_VERSION}"
COPY . .
# Remove the source's rust-toolchain.toml so the build uses the toolchain
# pinned by this image (RUST_VERSION ARG), matching what `deps` cooked with.
# The source file says `channel = "stable"`, which makes rustup override
# to a newer stable (e.g. 1.97) mid-build — changing the rustc hash and
# invalidating every fingerprint (full ~570-crate recompile).
RUN rm -f rust-toolchain.toml
# Build the embedded SPA before compiling the Rust binary. rust-embed bakes
# `frontend/dist` into the binary at compile time, so this must run first.
# Only the admin binary ships a frontend; for the others this is a no-op.
RUN if [ "${BINARY_NAME}" = "downloader-admin" ]; then \
      apk add --no-cache curl unzip \
      && curl -fsSL https://bun.sh/install | bash \
      && export BUN_INSTALL="$HOME/.bun" \
      && export PATH="$BUN_INSTALL/bin:$PATH" \
      && cd bins/downloader-admin/frontend \
      && bun install --frozen-lockfile \
      && bun run build \
      && cd /app; \
    fi
# Build and compress in a single RUN: the binary lives only in this step's
# view of /app/target, and upx must read it before it goes to the output path.
# No cache mounts here (neither /app/target nor /usr/local/cargo/registry):
# both are baked into the inherited `deps` layer, and mounting either would
# shadow those files. The concurrent `builder` stages then read the registry
# via copy-on-write, with no shared mutable state to race on.
RUN SKIP_FRONTEND_BUILD=1 \
  cargo build \
  --release \
  --target "${RUST_TARGET}" \
  --package "${BINARY_NAME}" \
  && upx --best --lzma \
  "/app/target/${RUST_TARGET}/release/${BINARY_NAME}" \
  -o "/${BINARY_NAME}"


##########
# Step 4 #
##########
##
## Static ffmpeg build, shared by bot and worker runtime stages.
##
FROM "${RUNNER_CONTAINER}" AS ffmpeg
RUN apk add --no-cache \
  curl \
  bzip2 \
  xz \
  ;
WORKDIR /ffmpeg
ARG DNS_SERVERS
ARG FFMPEG_DOWNLOAD_URL
RUN cd "$(mktemp --directory)" && \
  curl --dns-servers "${DNS_SERVERS}" -sL "${FFMPEG_DOWNLOAD_URL}" | tar xvJ \
  && cd ffmpeg-*-amd64-static \
  && mv ffmpeg ffprobe qt-faststart /ffmpeg/ \
  && cd .. \
  && rm -rf "$(pwd)"


##########
# Step 5 #
##########
##
## Common runtime base.
## Shared across all final runtime targets.
##
FROM "${RUNNER_CONTAINER}" AS runtime-base
RUN apk add --no-cache \
  bash \
  ;


##########
# bot    #
##########
##
## downloader-bot runtime image.
##
FROM runtime-base AS bot
ARG BINARY_NAME='downloader-bot'
# Install ffmpeg
COPY --from=ffmpeg /ffmpeg/* /usr/local/bin/
RUN ffmpeg -version
# Mock yt-dlp (bot does not run downloads)
RUN printf '#!/bin/bash\n\necho '\''yt-dlp is not installed'\''\nexit 1\n' > /usr/local/bin/yt-dlp \
  && chmod +x /usr/local/bin/yt-dlp
# Install app
WORKDIR /app
COPY --from=builder --chmod=755 "/${BINARY_NAME}" /usr/local/bin/
# Entrypoint
RUN printf '#!/bin/bash\n\n/usr/local/bin/%s "$@"\n' "${BINARY_NAME}" > /entrypoint.sh \
  && chmod +x /entrypoint.sh
LABEL maintainer="Josip Igrec <me@allypost.net>"
LABEL org.opencontainers.image.title="downloader-bot"
LABEL org.opencontainers.image.source="https://github.com/Allypost/downloader-hub"
LABEL org.opencontainers.image.licenses="MPL-2.0"
LABEL org.opencontainers.image.authors="Josip Igrec <me@allypost.net>"
EXPOSE 8080
ENTRYPOINT [ "/entrypoint.sh" ]


#############
# central   #
#############
##
## downloader-central runtime image.
##
FROM oven/bun:1-alpine AS central
ARG BINARY_NAME='downloader-central'
RUN apk add --no-cache \
  bash \
  ;
# Initialize migrations
WORKDIR /migrations
COPY ./crates/app-database/ ./
RUN bun install --frozen-lockfile
# Install app
WORKDIR /app
COPY --from=builder --chmod=755 "/${BINARY_NAME}" /usr/local/bin/
# Entrypoint
RUN printf '#!/usr/bin/env bash\nset -euo pipefail\ncd /migrations\nbunx convex deploy --verbose --codegen '\''disable'\''\ncd -\n/usr/local/bin/%s "$@"\n' "${BINARY_NAME}" > /entrypoint.sh \
  && chmod +x /entrypoint.sh
LABEL maintainer="Josip Igrec <me@allypost.net>"
LABEL org.opencontainers.image.title="downloader-central"
LABEL org.opencontainers.image.source="https://github.com/Allypost/downloader-hub"
LABEL org.opencontainers.image.licenses="MPL-2.0"
LABEL org.opencontainers.image.authors="Josip Igrec <me@allypost.net>"
EXPOSE 8080
ENTRYPOINT [ "/entrypoint.sh" ]
CMD [ "run" ]


##########
# worker #
##########
##
## downloader-worker runtime image.
##
FROM runtime-base AS worker
ARG BINARY_NAME='downloader-worker'
ARG RUN_USERNAME='app'
ARG RUN_USER_ID='1000'
ARG RUN_GROUP_ID='1000'
# Runtime deps for processing downloads
RUN apk add --no-cache \
  curl \
  python3 \
  imagemagick \
  imagemagick-jpeg \
  deno \
  ;
# Install ffmpeg
COPY --from=ffmpeg /ffmpeg/* /usr/local/bin/
RUN ffmpeg -version
# Install yt-dlp (pinned)
COPY ./.docker/update_yt-dlp.sh /usr/bin/update_yt-dlp.sh
ARG YT_DLP_VERSION
ENV YT_DLP_VERSION=${YT_DLP_VERSION}
RUN addgroup -g "${RUN_GROUP_ID}" "${RUN_USERNAME}" \
  && adduser -u "${RUN_USER_ID}" -G "${RUN_USERNAME}" -D "${RUN_USERNAME}" \
  && chmod a=rx /usr/bin/update_yt-dlp.sh \
  && /usr/bin/update_yt-dlp.sh \
  && chgrp "${RUN_USERNAME}" /usr/local/bin/yt-dlp \
  ;
# Install app
WORKDIR /app
COPY --from=builder --chmod=755 "/${BINARY_NAME}" /usr/local/bin/
# Entrypoint
RUN printf '#!/bin/bash\n\n/usr/local/bin/%s "$@"\n' "${BINARY_NAME}" > /entrypoint.sh \
  && chmod +x /entrypoint.sh
USER ${RUN_USERNAME}
LABEL maintainer="Josip Igrec <me@allypost.net>"
LABEL org.opencontainers.image.title="downloader-worker"
LABEL org.opencontainers.image.source="https://github.com/Allypost/downloader-hub"
LABEL org.opencontainers.image.licenses="MPL-2.0"
LABEL org.opencontainers.image.authors="Josip Igrec <me@allypost.net>"
ENTRYPOINT [ "/entrypoint.sh" ]
CMD [ "run" ]


##########
# admin  #
##########
##
## downloader-admin runtime image.
##
FROM runtime-base AS admin
ARG BINARY_NAME='downloader-admin'
# Install app
WORKDIR /app
COPY --from=builder --chmod=755 "/${BINARY_NAME}" /usr/local/bin/
# Entrypoint
RUN printf '#!/bin/bash\n\n/usr/local/bin/%s "$@"\n' "${BINARY_NAME}" > /entrypoint.sh \
  && chmod +x /entrypoint.sh
LABEL maintainer="Josip Igrec <me@allypost.net>"
LABEL org.opencontainers.image.title="downloader-admin"
LABEL org.opencontainers.image.source="https://github.com/Allypost/downloader-hub"
LABEL org.opencontainers.image.licenses="MPL-2.0"
LABEL org.opencontainers.image.authors="Josip Igrec <me@allypost.net>"
EXPOSE 8082
ENTRYPOINT [ "/entrypoint.sh" ]
CMD [ "run" ]
