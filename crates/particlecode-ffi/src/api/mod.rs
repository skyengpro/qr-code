//! The flutter_rust_bridge-style API.
//!
//! Written as plain Rust so `flutter_rust_bridge_codegen generate --rust-input crate::api`
//! can read it without this crate depending on the bridge:
//!
//! * opaque handles with methods: [`encoder::ParticleEncoder`] and
//!   [`decoder::ParticleDecoder`] (private fields, so the generator treats them as opaque);
//! * plain DTO structs and enums in [`dto`], all with public fields and no references;
//! * free functions for the motion curves ([`motion`]) and liveness ([`liveness`]);
//! * one error type, [`error::FfiError`].
//!
//! Conventions: lengths and counts are `u32`, times are seconds as `f64`, coordinates are
//! pitch units with `y` down, colors are palette indices (resolve them with
//! [`dto::palette`]).

// The bridge hands owned values over the boundary; by-value arguments are the API.
#![allow(clippy::needless_pass_by_value)]

pub mod decoder;
pub mod dto;
pub mod encoder;
pub mod error;
pub mod liveness;
pub mod motion;
