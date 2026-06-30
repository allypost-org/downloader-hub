set dotenv-load
set positional-arguments

default:
    @just --list

run package *args:
    APPLICATION_NAME='{{ package }}' \
    cargo run \
        --release \
        --bin '{{ package }}' \
        -- "$@" \

build bin:
    APPLICATION_NAME='{{ bin }}' \
    cargo build \
        --release \
        --bin '{{ bin }}' \
        --timings

dev-run package *args:
    shift; \
    APPLICATION_NAME='{{ package }}' \
    cargo run \
        --package '{{ package }}' \
        -- "$@" \

dev-build package *args:
    shift; \
    APPLICATION_NAME='{{ package }}' \
    cargo build \
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
    && bun run dev \

fmt-dev: lint-fix && fmt
    rustup run nightly cargo fmt --all \

lint:
    cargo clippy \
        --workspace \
        --all-features \
        -- \

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

[parallel]
docker-build-all: docker-build-downloader-central docker-build-downloader-worker docker-build-downloader-bot

docker-build tag dockerfile_name *args:
    shift; \
    shift; \
    docker build \
        --progress plain \
        -t '{{ tag }}' \
        -f ./.docker/'{{ dockerfile_name }}'/Dockerfile \
        "$@" \
        . \

docker-build-downloader-central:
    just docker-build 'allypost/downloader-central' 'downloader-central'

docker-build-downloader-worker:
    just docker-build 'allypost/downloader-worker' 'downloader-worker'

docker-build-downloader-bot:
    just docker-build 'allypost/downloader-bot' 'downloader-bot'

[parallel]
docker-push-all: (docker-push 'allypost/downloader-central') (docker-push 'allypost/downloader-worker') (docker-push 'allypost/downloader-bot')

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
