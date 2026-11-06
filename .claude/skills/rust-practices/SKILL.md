---
name: rust-practices
description: Rust coding conventions for this workspace — error handling, no_std rules for particlecode-core, numeric/geometry code, testing with proptest, docs and lints. Use whenever writing or reviewing Rust code in this repo.
---

# Rust practices for Particle Code

## Crate boundaries

| Crate | May depend on | Rules |
| --- | --- | --- |
| `particlecode-core` | `alloc`, `libm`, `raptorq`, RustCrypto, `thiserror` | `no_std` + `alloc`. No `std::`, no floats via `f64::sin` (use `libm`), no I/O, no pixels. `std` feature only adds OS randomness. |
| `particlecode-vision` | core | The only crate that reads pixels. No rendering. |
| `particlecode-render` | core, tiny-skia, png, gif | The only crate that writes pixels. |
| `particlecode-shapes` / `-cli` | everything | Binaries; `anyhow` allowed here and only here. |
| `particlecode-ffi` | core, vision | Only crate allowed `unsafe` (`feed_camera_frame` reads camera planes by address; generated frb code). Every `unsafe` block gets a `// SAFETY:` comment. |

Never introduce a dependency cycle; tests that need rendering use `particlecode-render` as a
**dev-dependency**.

## Errors

* Libraries return typed errors (`thiserror` enums: `DecodeError`, `EncodeError`, ...).
  No `anyhow` in libraries, no `Box<dyn Error>`.
* No `unwrap()`/`expect()` on data that comes from outside (camera frames, headers,
  files). `expect("why this cannot fail")` is fine for true invariants.
* Decoding never panics on any input: every per-frame step either produces output or
  rejects the frame. Fuzz targets enforce this for the header parser.

## Numeric and geometry code

* All geometry comes from `particlecode_core::layout` (pitch units, y down). Do not
  hard-code positions or radii anywhere else.
* Casts between float and int are allowed (the pedantic cast lints are off) but clamp
  before casting pixel coordinates.
* Determinism matters: encoder output must be bit-identical across platforms. Avoid
  `HashMap` iteration order and platform-dependent float functions in anything that feeds
  the encoder; use `libm`.

## API design

* Plain data (`FrameSpec`, `Header`, `Sym3`) derives `Debug, Clone, PartialEq` and
  `serde` behind the `serde` feature.
* Prefer constructors returning `Result` over panicking validation.
* Public items are documented (`missing_docs` warns). Doc comments state units.
* Keep functions small enough to unit test; put pure math in free functions.

## Testing

* Unit tests live next to the code (`#[cfg(test)] mod tests`).
* Codec layers get **property tests** (`proptest`): encode → corrupt within the correction
  bound → decode must round-trip; beyond the bound it must fail cleanly, never panic.
* Vision is tested on synthetic renders (`particlecode-render::synth`): blur, perspective,
  noise, glare. Keep test images small (≤ 600 px) so `cargo test` stays fast.
* Long-running tests are `#[ignore]` with a comment saying how to run them.

## Style

* `cargo fmt` is law; `rustfmt.toml` sets width 100.
* Clippy runs with `-D warnings` including `pedantic` (see `[workspace.lints]`). Allow a
  lint locally with `#[allow(clippy::xyz)] // reason` rather than globally.
* Match the surrounding code's comment density; comments explain *why*, not *what*.
