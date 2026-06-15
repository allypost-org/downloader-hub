# syntax=docker/dockerfile:1
ARG RUST_VERSION='1.94'
ARG RUST_TARGET='x86_64-unknown-linux-musl'
ARG BINARY_NAME='downloader-central'

ARG APP_FEATURES=''

ARG RUNNER_CONTAINER="oven/bun:1-alpine"

ARG DNS_SERVERS='9.9.9.9,8.8.8.8,1.1.1.1'

##########
# Step 0 #
##########
##
## Setup base image with cargo-chef
##
FROM rust:${RUST_VERSION}-alpine AS chef
# `curl` and `bash` are needed for cargo-binstall
# `musl-tools` and `musl-dev` are needed to build app with musl target
RUN update-ca-certificates
RUN apk update && apk add \
  curl \
  bash \
  jq \
  clang
# Install cargo-chef
RUN cargo install cargo-chef --locked
# Add proper target and compile flags
ARG RUST_TARGET
RUN rustup target add "${RUST_TARGET}"
ENV RUSTFLAGS='-C target-feature=+crt-static'
WORKDIR /app


##########
# Step 1 #
##########
##
## Generate a build plan for rust dependencies
##
FROM chef AS planner
WORKDIR /chef
# Copy all files except in the bins folder
COPY --exclude=./bins/**/* . .
# Add only the binary we need
ARG BINARY_NAME
COPY ./bins/${BINARY_NAME}/ ./bins/${BINARY_NAME}/
# Generate "lockfile" aka dependency dump
RUN cargo chef prepare \
  --recipe-path recipe.json


##########
# Step 2 #
##########
##
## Build app with the cached dependencies
##
FROM chef AS builder
RUN update-ca-certificates
# Install upx - https://upx.github.io/
ARG DNS_SERVERS
RUN cd "$(mktemp --directory)" && \
  curl --dns-servers "${DNS_SERVERS}" -sL "$(\
  curl --dns-servers "${DNS_SERVERS}" -sL https://api.github.com/repos/upx/upx/releases \
  | jq -r '.[0].assets | .[] | select(.name | test("amd64_linux")) | .browser_download_url' \
  | head -n1\
  )" | tar xvJ  && \
  cd * && \
  mv upx /usr/bin && \
  cd .. && \
  rm -rf "$(pwd)" && \
  echo "Installed upx"
# Build dependencies
ARG RUST_TARGET
ARG APP_FEATURES
ARG BINARY_NAME
COPY --from=planner /chef/recipe.json recipe.json
ENV APPLICATION_NAME=${BINARY_NAME}
RUN cargo chef cook \
  --release \
  --target "${RUST_TARGET}" \
  --features "${APP_FEATURES}" \
  --package "${BINARY_NAME}" \
  --recipe-path recipe.json
ARG RUST_TARGET
RUN rustup target add "${RUST_TARGET}"
# Copy rest of files and compile
# only the remaining app code
ARG RUST_TARGET
ARG APP_FEATURES
ARG BINARY_NAME
# Copy all files except in the bins folder
COPY --exclude=./bins/**/* . .
# Add only the binary we need
COPY ./bins/${BINARY_NAME}/ ./bins/${BINARY_NAME}/
ENV APPLICATION_NAME=${BINARY_NAME}
RUN cargo build \
  --release \
  --target "${RUST_TARGET}" \
  --features "${APP_FEATURES}" \
  --package "${BINARY_NAME}" \
  && upx --best --lzma \
  "/app/target/${RUST_TARGET}/release/${BINARY_NAME}" \
  -o /"${BINARY_NAME}" \
  ;

##########
# Step 3 #
##########
##
## Run the app in a configured environment
##
FROM "${RUNNER_CONTAINER}"
RUN apk add \
  bash \
  ;
# Initialize migrations
WORKDIR /migrations
COPY ./crates/app-database/ ./
RUN bun install --frozen-lockfile
# Install app
WORKDIR /app
ARG BINARY_NAME
COPY --from=builder --chmod=755 "/${BINARY_NAME}" /usr/local/bin/
# Run app
RUN echo -e "#!/usr/bin/env bash\nset -euo pipefail\ncd /migrations\nbunx convex deploy --verbose --codegen 'disable'\ncd -\n/usr/local/bin/${BINARY_NAME} \"\$@\"" > /entrypoint.sh && chmod +x /entrypoint.sh
LABEL maintainer="Josip Igrec <me@allypost.net>"
LABEL org.opencontainers.image.title=${BINARY_NAME}
LABEL org.opencontainers.image.source="https://github.com/Allypost/downloader-hub"
LABEL org.opencontainers.image.licenses="MPL-2.0"
LABEL org.opencontainers.image.authors="Josip Igrec <me@allypost.net>"
EXPOSE 8080
ENTRYPOINT [ "/entrypoint.sh" ]
CMD [ "run" ]

