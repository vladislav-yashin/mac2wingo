# AGENTS.md

Guidance for agents working in this repository.

## Always rebuild after finishing a feature

**After completing any feature or user-visible change, always rebuild the `.app`
bundle before reporting done:**

```sh
./scripts/build-universal.sh
```

The application is distributed as `dist/Mac2Win Go.app`; source changes alone are
not visible to the user. A stale `dist/` is a bug. The build also verifies that
the universal (arm64 + x86_64) binary links, so run it even when only Rust code
changed. Helper tools under `vendor/tools` do not need to be rebuilt unless the
change touches `scripts/fetch-tools.sh`.

## Quality checks

Run all of these before declaring a feature complete:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

There is also a `justfile` (`just check`, `just test`, `just lint`).
