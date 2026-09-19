default:
    @just --list

run:
    cargo run

build:
    cargo build

release:
    cargo build --release

test:
    cargo test --all-features

lint:
    cargo clippy --all-targets --all-features -- -D warnings

fmt:
    cargo fmt --all

check: fmt
    cargo fmt --all -- --check
    just lint
    just test

universal:
    ./scripts/build-universal.sh

dmg:
    ./scripts/build-dmg.sh

icon:
    ./scripts/generate-icon.sh

dmg-bg:
    ./scripts/generate-dmg-background.sh
