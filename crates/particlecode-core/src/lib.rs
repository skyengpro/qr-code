//! # particlecode-core
//!
//! The camera-free half of Particle Code: everything that turns payload bytes into
//! [`FrameSpec`]s and symbol streams back into payload bytes.
//!
//! * [`profile`] — the S / M / L profiles and the palette.
//! * [`layout`] — frame geometry in units of the dot pitch `p`: anchors, halos, the hex
//!   lattice, keep-outs and the serpentine dot order.
//! * [`shapes`] — masks over the lattice (disc, letters, custom tables).
//! * [`gf`], [`rs`], [`crc`] — finite fields, Reed-Solomon and checksums.
//! * [`header`] — the 96-bit frame header and its 48-dot code.
//! * [`body`] — packing, whitening, RS protection and interleaving of the dot field.
//! * [`session`] / [`receiver`] — segmented RaptorQ fountain coding end to end.
//! * [`liveness`] — particle angles carrying the packet seed and the rolling HMAC value.
//! * [`motion`] — deterministic animation "moods" shared by every painter.
//!
//! The crate is `no_std` + `alloc`; the `std` feature only adds OS randomness and
//! `std::error::Error` impls. See `docs/spec.md` and `docs/design.md` for the format.
#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod body;
pub mod crc;
pub mod error;
pub mod frame;
pub mod gf;
pub mod header;
pub mod layout;
pub mod liveness;
pub mod motion;
pub mod profile;
pub mod receiver;
pub mod rs;
mod segment;
pub mod session;
pub mod shapes;
pub mod symbol;

pub use error::{DecodeError, EncodeError};
pub use frame::FrameSpec;
pub use header::Header;
pub use liveness::Liveness;
pub use profile::{EccLevel, Palette, Profile};
pub use receiver::{PacketReceiver, Progress, Receiver};
pub use session::{
    DisplayInfo, PacketSession, Session, SessionConfig, ShapeChoice, choose_profile,
};
pub use symbol::Sym3;
