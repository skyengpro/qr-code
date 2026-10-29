# Particle Code

Aesthetic dense optical code: COSE/CBOR payloads move from a phone screen to a phone
camera as an animated field of colored dots. Rust workspace; the Flutter and Tauri apps
consume it through `particlecode-ffi`.

* Product spec: `docs/spec.md`. Implementation decisions and spec corrections:
  `docs/design.md`. Read both before changing the format.
* Skills in `.claude/skills/`: `rust-practices` (code conventions), `cargo-workflow`
  (commands and pre-commit checks), `cargo-deny` (dependency policy),
  `particlecode-format` (format invariants and change checklist).

## Layout

```
crates/particlecode-core     no_std + alloc: layout, header, RS, body codec, RaptorQ sessions, motion
crates/particlecode-vision   camera image -> header + symbols (locate, read)
crates/particlecode-render   FrameSpec -> pixels (tiny-skia), PNG/GIF/SVG packagers, synthetic degradations
crates/particlecode-shapes   shape tool (SVG path -> lattice bitmaps, checks, validate, shapes.bin)
crates/particlecode-cli      `particlecode` binary + examples/
crates/particlecode-ffi      flutter_rust_bridge-style API + C ABI
examples/                    runnable end-to-end examples (see examples/README.md)
docs/                        spec, design notes, README images
```

## Must-run before committing

```sh
cargo fmt --all --check && cargo lint && cargo t && cargo deny check
```

## Rules

* Geometry only from `particlecode_core::layout`. Palette only from `profile::colors`.
* Core stays `no_std`; check with `cargo build -p particlecode-core --no-default-features`.
* Decoders never panic on input; they reject.
* Format changes bump `FORMAT_VERSION` / `SHAPE_TABLE_VERSION` and update `docs/design.md`.
