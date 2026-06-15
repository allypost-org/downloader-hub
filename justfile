set dotenv-load
set positional-arguments

rustflags := "-C target-feature=+crt-static"
# rust_target := "x86_64-unknown-linux-gnu"
rust_target := "x86_64-unknown-linux-musl"

default:
    @just --list

build bin:
    RUSTFLAGS='{{ rustflags }}' \
    APPLICATION_NAME='{{ bin }}' \
    cargo build --release --bin '{{ bin }}' --target '{{ rust_target }}' --timings

dev *args: (dev-watch-server args)

dev-watch package *args:
    shift; \
    RUSTFLAGS='{{ rustflags }}' \
    APPLICATION_NAME='{{ package }}' \
    watchexec \
        --clear=reset \
        --restart \
        --debounce '500ms' \
        --watch './crates' \
        --watch './bins' \
        --ignore 'crates/app-migration/**/*' \
        --stop-signal 'kill' \
        -- \
          just dev-run "$@"

dev-watch-server *args: (dev-watch 'downloader-hub' args)

dev-watch-cli *args: (dev-watch 'downloader-cli' args)

dev-watch-telegram-bot *args: (dev-watch 'downloader-telegram-bot' args)

dev-watch-build package:
    RUSTFLAGS='{{ rustflags }}' \
    APPLICATION_NAME='{{ package }}' \
    cargo watch \
        --clear \
        --quiet \
        --watch './crates' \
        --watch './bins' \
        --ignore 'crates/app-migration/**/*' \
        --exec "build --target '{{ rust_target }}' --package '{{ package }}'" \

dev-watch-build-server: (dev-watch-build 'downloader-hub')

dev-watch-build-cli: (dev-watch-build 'downloader-cli')

dev-watch-build-telegram-bot: (dev-watch-build 'downloader-telegram-bot')

dev-run package *args:
    shift; \
    RUSTFLAGS='{{ rustflags }}' \
    APPLICATION_NAME='{{ package }}' \
    cargo run \
        --target '{{ rust_target }}' \
        --package '{{ package }}' \
        -- "$@" \

dev-run-server *args: (dev-run 'downloader-hub' args)

dev-run-cli *args: (dev-run 'downloader-cli' args)

dev-run-telegram-bot *args: (dev-run 'downloader-telegram-bot' args)

migrate +ARGS: && generate-entities
    cd ./crates/app-migration \
    && cargo run -- "$@" \

migrate-up:
    just migrate up

migration-create migration_name:
    just migrate generate '{{ migration_name }}'

generate-entities:
    sea-orm-cli generate entity \
        --with-copy-enums \
        --with-serde 'serialize' \
        --model-extra-attributes 'serde(rename_all = "camelCase")' \
        --serde-skip-hidden-column \
        --output-dir "./crates/app-entities/src/entities" \

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
    cargo fmt --all \

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

db-dev:
    cd ./crates/app-database \
    && bun run dev \
    && cd ../.. \

install-cli:
    RUSTFLAGS='{{ rustflags }}' \
    cargo install \
        --path=./bins/downloader-cli \
        --profile=release-cli \
        --target='{{ rust_target }}' \
    && if [ -n "${INSTALL_LOCATION:-}" ]; then \
        mv "$HOME/.cargo/bin/downloader-cli" "$INSTALL_LOCATION"; \
    fi \
