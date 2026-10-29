//! # particlecode-vision
//!
//! The camera half of Particle Code: from a camera image to header symbols, body symbols
//! and particle sectors. It is the only crate that reads pixels.
//!
//! ```no_run
//! use particlecode_core::Profile;
//! use particlecode_core::layout::{DotOrder, Lattice};
//! use particlecode_vision::{YuvFrame, locate, read};
//!
//! # fn camera_frame() -> YuvFrame { YuvFrame::default() }
//! // The dot order of (profile, shape id); the shape table lives in `particlecode-core`.
//! let order_for = |profile: Profile, shape: u8| {
//!     (shape == 0).then(|| {
//!         let lattice = Lattice::new(profile);
//!         let f = profile.field_half();
//!         let mask: Vec<bool> = lattice.points().iter().map(|p| p.at.norm() <= f).collect();
//!         DotOrder::new(&lattice, &mask)
//!     })
//! };
//! let frame = camera_frame();
//! if let Some(lock) = locate(&frame.y) {
//!     if let Ok((header, symbols, liveness)) = read(&frame, &lock, &order_for) {
//!         // header.packet_seed, symbols with confidences for `BodyCodec::decode`, ...
//!         # let _ = (header, symbols, liveness);
//!     }
//! }
//! ```
//!
//! * [`locate`] finds the three hero anchors on the luma plane, decides the profile and
//!   the orientation and returns a [`Lock`] with a homography already refined against the
//!   calibration swatches. A few milliseconds per 1080p frame.
//! * [`read_raw`] / [`read`] calibrate the four hues from the swatches, classify the header
//!   dots, and — once the header has chosen the dot order — every body dot, then read the
//!   particles. [`read_with_order`] does the same for a caller that decodes the header
//!   itself. Symbols carry a confidence; those below
//!   `particlecode_core::rs::ERASURE_BELOW` become Reed-Solomon erasures downstream.
//! * [`sharpness`] scores a frame so the caller can keep the sharpest of the ~3 camera
//!   captures of each data frame.
//!
//! All geometry comes from `particlecode_core::layout`; nothing here hard-codes positions.
//! See `docs/spec.md` (decode pipeline) and `docs/design.md`.
//!
//! ## What the decoder does, and where it departs from the spec's sketch
//!
//! * **Anchors.** Adaptive threshold (`pixel < α · local mean`, integral image), run-based
//!   8-connected labelling, ring-with-core pairs (or blur-merged discs) as candidates, triples
//!   with equal sides. Profile and rotation come from the *white and black swatches of the top
//!   halo*, which are mirror-symmetric about the vertical axis: a mirrored (selfie) view locks
//!   like any other and [`read_with_order`] notices from the hue order and mirrors the
//!   homography. A lock is accepted only if a stricter swatch/header-ring test passes on the
//!   refined homography *and* fails on the halos displaced by a pitch, which is what keeps
//!   noise and texture from locking.
//! * **Homography.** Anchors, center mark, swatches and header dots give a first fit (rejected
//!   above 0.15 pitch RMS, the spec's bound). Once the dot order is known, the fit is extended
//!   with *lattice-phase measurements*: the Fourier phase of the ink image at the three
//!   reciprocal lattice vectors, over ~150 windows spread across the field. It measures how far
//!   the lattice is displaced, independent of which dots are large or what colour they are;
//!   per-dot centroids, which the spec sketches, are pulled towards wherever the search starts
//!   once blur makes neighbours overlap. Lens distortion, tilt and curved screens are absorbed
//!   by the per-dot centroid refinement (≤ 0.3 pitch) that precedes every measurement.
//! * **Colour.** The paper level is fitted with a plane over the particle band and divided out
//!   (lighting gradients); the white-balance offset is the paper's own chroma. Hues are
//!   classified by the *direction* in the `CbCr` plane against the four swatch centroids
//!   (blur and dot size shrink a dot's chroma towards zero without rotating it).
//! * **Size.** Each dot's ink mass (`Σ bg − luma`, blur-invariant) is measured in two windows
//!   and normalised by the swatch of the same hue and by the local pitch² (perspective). A
//!   Fisher discriminant fitted to the frame's own two clusters picks the best combination for
//!   the blur at hand; boundaries follow slow variations of scale across the field; and the
//!   spec's neighbour-median rule decides where the neighbourhood is clear.
//! * **Confidence** is the weakest of three independent factors: hue margin (and chroma
//!   strength), distance of the size score from its boundary, and ink depth (glare and shadow
//!   wash dots out).

mod anchor;
mod calib;
mod detect;
mod error;
mod geometry;
mod homography;
mod image;
mod lattice;
mod liveness;
mod particles;
mod read;
mod sample;
mod sharp;
mod size;
mod util;

pub use calib::Calibration;
pub use detect::{Lock, locate};
pub use error::VisionError;
pub use homography::Homography;
pub use image::{Plane, PlaneRef, YuvFrame, YuvRef};
pub use liveness::Liveness;
pub use read::{RawRead, read, read_raw, read_with_order};
pub use sharp::sharpness;
