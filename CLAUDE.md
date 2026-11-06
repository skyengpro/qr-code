# Particle Code

Aesthetic dense optical code: COSE/CBOR payloads move from a phone screen to a phone
camera as an animated field of colored dots. Rust workspace; the Flutter plugin
(`flutter/particle_code`) consumes it through `particlecode-ffi` and flutter_rust_bridge only.

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
crates/particlecode-ffi      flutter_rust_bridge API (generated: src/frb_generated.rs); built by the plugin's hook
flutter/particle_code        Flutter plugin: Dart sender + scanner, native camera, hook/build.dart (Native Assets)
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
* Rust reaches Flutter only through flutter_rust_bridge. After changing `particlecode-ffi/src/api`,
  run `flutter_rust_bridge_codegen generate` in `flutter/particle_code` and commit the output.
* Format changes bump `FORMAT_VERSION` / `SHAPE_TABLE_VERSION` and update `docs/design.md`.
