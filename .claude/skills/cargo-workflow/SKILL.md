---
name: cargo-workflow
description: How to build, test, lint, document, benchmark and fuzz this Cargo workspace, and the exact checks to run before committing. Use before any commit or PR, when CI fails, or when unsure which cargo command to run.
---

# Cargo workflow

The toolchain is pinned in `rust-toolchain.toml`; `rustup` picks it up automatically.
Aliases live in `.cargo/config.toml`.

## Before every commit (what CI runs)

```sh
cargo fmt --all --check
cargo lint                  # = clippy --workspace --all-targets --all-features -- -D warnings
cargo t                     # = test --workspace --all-features
cargo docs                  # = doc --workspace --no-deps --all-features (RUSTDOCFLAGS=-D warnings in CI)
cargo deny check            # licenses, advisories, bans, sources — see the cargo-deny skill
cargo build -p particlecode-core --no-default-features   # no_std + alloc must keep building
```

Fix warnings rather than allowing them. If one of these fails, do not commit.

## Everyday commands

| Goal | Command |
| --- | --- |
| Test one crate | `cargo test -p particlecode-core` |
| Test one test | `cargo test -p particlecode-vision -- locate::tests::tilted` |
| Show test output | append `-- --nocapture` |
| Slow/ignored tests | `cargo test --workspace --release -- --ignored` |
| Run the CLI | `cargo pc encode payload.cbor -o out/` (alias for `run -p particlecode-cli --release --`) |
| Run an example | `cargo run --release -p particlecode-cli --example roundtrip` |
| Shapes tool | `cargo run --release -p particlecode-shapes -- list` |
| Dependency tree | `cargo tree -p particlecode-core -e normal` |
| Why is X here | `cargo tree -i <crate>` |
| Fuzz (nightly) | `cd fuzz && cargo +nightly fuzz run header` |

## Adding a dependency

1. Add it once to `[workspace.dependencies]` in the root `Cargo.toml` with an exact
   minimum version (`"1.2.3"`, never `"*"`), `default-features = false` when the crate is
   used from `particlecode-core`.
2. Reference it from the member with `name.workspace = true` (plus `features = [...]`).
3. Run `cargo deny check` — new licenses or duplicate versions show up here.
4. Commit `Cargo.lock` (this workspace ships binaries).

## Release builds

`[profile.release]` uses thin LTO and one codegen unit. Dependencies build at
`opt-level = 2` even in dev so RaptorQ and tiny-skia tests are not painfully slow.
