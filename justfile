set dotenv-load
set positional-arguments

# rustup's cargo must win over a distro cargo: only it honours
# rust-toolchain.toml and has the pinned musl std. Prepending is safe even
# when rustup isn't installed - the dir just falls out of PATH lookup.
#
cargo_bin := env('CARGO_HOME', env('HOME', '') + '/.cargo') + '/bin'
export PATH := cargo_bin + ':' + env('PATH')

default:
    @just --list

run package *args:
    APPLICATION_NAME='{{ package }}' \
    CARGO_TARGET_DIR='target/{{ package }}' \
    cargo run \
        --release \
        --bin '{{ package }}' \
        -- "$@" \

build-all: (build "downloader-cli") (build "downloader-central") (build "downloader-bot") (build "downloader-worker")

build bin:
    APPLICATION_NAME='{{ bin }}' \
    CARGO_TARGET_DIR='target/{{ bin }}' \
    cargo build \
        --release \
        --bin '{{ bin }}' \
        --timings

dev-run package *args:
    shift; \
    APPLICATION_NAME='{{ package }}' \
    CARGO_TARGET_DIR='target/{{ package }}' \
    cargo run \
        --package '{{ package }}' \
        -- "$@" \

dev-build package *args:
    shift; \
    APPLICATION_NAME='{{ package }}' \
    CARGO_TARGET_DIR='target/{{ package }}' \
    cargo build \
        --profile dev \
        --package '{{ package }}' \
        "$@"

dev-watch package *args:
    shift; \
    just _watch just dev-run '{{ package }}' "$@"

dev-watch-build package *args:
    shift; \
    just _watch just dev-build '{{ package }}' "$@"

_watch *args:
    watchexec \
        --clear=reset \
        --restart \
        --debounce '500ms' \
        --watch './crates' \
        --watch './bins' \
        --stop-signal 'kill' \
        -- "$@"

db-dev:
    cd ./crates/app-database \
    && bun install \
    && bun run dev \

db-codegen:
    cd ./crates/app-database \
    && bun run convex codegen \

fmt-dev: lint-fix && fmt
    rustup run nightly cargo fmt --all \

lint:
    cargo clippy \
        --workspace \
        --all-features \
        -- \
        -D warnings \

lint-fix:
    cargo clippy \
        --fix \
        --allow-dirty \
        --allow-staged \
        --workspace \
        --all-features \
        -- \

fmt:
    cargo fmt --all 2>/dev/null \

docker-build-all *args:
    docker buildx bake \
        --load \
        "$@"

docker-build target *args:
    shift; \
    docker buildx bake \
        --load \
        '{{ target }}' \
        "$@"

[parallel]
docker-push-all: (docker-push 'allypost/downloader-central') (docker-push 'allypost/downloader-worker') (docker-push 'allypost/downloader-bot') (docker-push 'allypost/downloader-admin')

docker-push tag *args:
    shift; \
    docker push '{{ tag }}' "$@"

docker-release-all: docker-build-all docker-push-all

install-cli:
    cargo install \
        --path=./bins/downloader-cli \
        --profile=release-cli \
    && if [ -n "${INSTALL_LOCATION:-}" ]; then \
        mv "$HOME/.cargo/bin/downloader-cli" "$INSTALL_LOCATION"; \
    fi \
