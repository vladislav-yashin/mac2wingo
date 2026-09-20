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

## Signing and notarization

`./scripts/build-universal.sh` and `./scripts/build-dmg.sh` sign their output
through `scripts/sign-and-notarize.sh`. With `MAC2WINGO_SIGN_IDENTITY` unset they
fall back to ad-hoc signing, which is fine for local work. To produce a
distributable build, export the Developer ID identity and notarization
credentials before running them:

```sh
export MAC2WINGO_SIGN_IDENTITY="Developer ID Application: Name (TEAMID)"
export APPLE_API_KEY=/path/to/AuthKey.p8 APPLE_API_KEY_ID=... APPLE_API_ISSUER=...
./scripts/build-universal.sh
./scripts/build-dmg.sh
```

`sign-and-notarize.sh --skip-notarize` signs without contacting Apple. The
release workflow sets the same variables from repository secrets; see README's
"Signing, notarization and Gatekeeper".

