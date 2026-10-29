//! # particlecode-shapes
//!
//! The shape tool of Particle Code: turns an SVG path into the per-profile lattice masks of
//! a custom shape, tells whether the result will decode (spec "Shapes", rules 2 to 6), packs
//! shapes into a `shapes.bin`, renders previews, and holds the synthetic half of rule 7.
//!
//! | Module | Job |
//! |--------|-----|
//! | [`path`], [`svg`] | SVG path data and `.svg` files to a `tiny_skia::Path` in the 1000 x 1000 box |
//! | [`raster`] | fill or stroke it into a raster, sample the S/M/L lattices |
//! | [`thickness`] | distance transform and openings: how wide are the strokes |
//! | [`rules`] | ink coverage, stroke width, islands, bytes per frame |
//! | [`report`] | text and JSON rendering of a [`rules::Report`] |
//! | [`build`] | refuse bad ids and failing shapes, insert into a `ShapeTable` |
//! | [`preview`] | demo frames of a mask, rendered by `particlecode-render` |
//! | [`validate`] | synthetic degraded frames and the pass/fail verdict of rule 7 |
//! | [`cli`] | the `clap` subcommands used by the binary |
//!
//! Geometry comes from `particlecode_core::layout`; nothing here defines lattice positions.

pub mod build;
pub mod cli;
pub mod error;
pub mod path;
pub mod preview;
pub mod raster;
pub mod report;
pub mod rules;
pub mod svg;
pub mod thickness;
pub mod validate;

pub use error::Error;
pub use raster::ShapeInput;
pub use rules::{Report, check};
