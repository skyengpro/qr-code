//! # particlecode-render
//!
//! The only crate that writes pixels. One painter turns a [`FrameSpec`](particlecode_core::FrameSpec) into a
//! [`tiny_skia::Pixmap`]; several packagers wrap it for the different outputs:
//!
//! | Use | Function | Output |
//! |-----|----------|--------|
//! | single frame | [`render`], [`render_animated`], [`encode_png`] | pixmap, PNG bytes |
//! | email / screen-to-camera | [`write_gif`] | looping animated GIF |
//! | asset store, video job | [`write_sequence`] | `frame_00000.png`... + `manifest.json` |
//! | print | [`render_svg`] | vector SVG |
//!
//! All of them take the plain-data frame description plus a [`RenderConfig`]; none needs a
//! session. The [`synth`] module degrades rendered images the way a camera would (blur,
//! perspective, noise, glare, exposure, colour cast) for vision tests, and [`demo`] builds
//! complete frames without the fountain coder.
//!
//! Geometry comes from [`particlecode_core::layout`] and colours from
//! [`particlecode_core::profile::colors`]; nothing is hard-coded here. Dots are
//! anti-aliased flat circles with no stroke or gradient.

mod color;
mod config;
pub mod demo;
mod error;
mod gif_out;
mod paint;
mod png_out;
mod sequence;
mod svg;
pub mod synth;

pub use config::RenderConfig;
pub use error::RenderError;
pub use gif_out::{GifOptions, gif_palette, write_gif, write_gif_with};
pub use paint::{Anim, render, render_anim, render_animated};
pub use png_out::encode_png;
pub use sequence::{Manifest, ManifestFrame, write_sequence};
pub use svg::render_svg;
