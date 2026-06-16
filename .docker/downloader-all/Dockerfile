# syntax=docker/dockerfile:1
ARG RUST_VERSION='1.96'
ARG RUST_TARGET='x86_64-unknown-linux-musl'

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
  parallel \
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
COPY . .
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
COPY --from=planner /chef/recipe.json recipe.json
RUN cargo chef cook \
  --release \
  --target "${RUST_TARGET}" \
  --recipe-path recipe.json
ARG RUST_TARGET
RUN rustup target add "${RUST_TARGET}"
# Copy rest of files and compile
# only the remaining app code
ARG RUST_TARGET
COPY . .
# RUN for p in ./bins/*; do cargo install --target="${RUST_TARGET}" --path="$p"; done
RUN cargo build \
  --release \
  --target "${RUST_TARGET}" \
  # && upx --best --lzma \
  # "/app/target/${RUST_TARGET}/release/${BINARY_NAME}" \
  # -o /"${BINARY_NAME}" \
  ;
RUN \
  mkdir -p /release/compressed \
  ; \
  for f in "./target/${RUST_TARGET}/release/"*.d; do \
  bin_name="$(basename "$f" '.d')"; \
  bin_dir="$(dirname "$f")"; \
  bin_path="$bin_dir/$bin_name"; \
  if [ ! -f "$bin_path" ]; then \
  continue; \
  fi \
  ; \
  echo "|> Got binary $bin_name"; \
  cp -a "$bin_path" "/release/"; \
  cp -a "$bin_path" "/release/compressed/"; \
  done \
  ; \
  upx --best --lzma \
  "/release/compressed"/* \
  ;


##########
# Step 3 #
##########
##
## Copy built binaries to a scratch image
##
FROM scratch
WORKDIR /
COPY --from=builder /release/ ./
