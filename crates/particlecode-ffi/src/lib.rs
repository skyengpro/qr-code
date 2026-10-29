//! # particlecode-ffi
//!
//! The foreign-function surface of Particle Code: what the Flutter app (through
//! `flutter_rust_bridge` v2) and the Tauri app (as a plain Rust dependency, or through the
//! C ABI) call. All logic lives in `particlecode-core` (and, from phase 2,
//! `particlecode-vision`); this crate only reshapes it into types that cross a language
//! boundary cheaply.
//!
//! * [`api`] — flutter_rust_bridge-style API: opaque handles with methods
//!   ([`api::encoder::ParticleEncoder`], [`api::decoder::ParticleDecoder`]) and plain DTOs.
//!   The crate does **not** depend on `flutter_rust_bridge`; the Flutter repository runs
//!   the code generator against this module:
//!
//!   ```text
//!   flutter_rust_bridge_codegen generate \
//!       --rust-input crate::api \
//!       --rust-root <path to crates/particlecode-ffi> \
//!       --dart-output lib/src/rust
//!   ```
//!
//! * [`capi`] — a small C ABI over the same handles, declared by hand in
//!   `include/particlecode.h` (no cbindgen). The only module that uses `unsafe`.
//! * [`json`] — the serde JSON form of a [`FrameSpec`](particlecode_core::FrameSpec), for
//!   painters that prefer to parse text (a Tauri webview, a debugging session).
//!
//! The README next to `Cargo.toml` documents the painter contract (coordinates, colors,
//! draw order, timers).
//!
//! # Phase 2
//!
//! Camera decoding (`ParticleDecoder::feed_yuv`) is not wired yet because it needs
//! `particlecode-vision`; see the marked spot in [`api::decoder`].

pub mod api;
pub mod capi;
pub mod json;
