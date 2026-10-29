//! [`FrameSpec`]: the plain-data description of one frame that every painter draws.
//!
//! Everything is in pitch units, origin at the frame center, `y` down. A painter scales by
//! `size_px / (2 * frame_half_side + 2 * quiet_zone)` and draws, back to front: background,
//! anchors, swatches, header dots, data dots, particles. Colors are indices into
//! [`crate::profile::colors`].

use alloc::vec::Vec;

use crate::header::Header;
use crate::layout::{self, DotOrder, ParticleBand, Point, Swatch, dims};
use crate::profile::Profile;
use crate::symbol::Sym3;

/// A ring-and-core anchor mark.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AnchorSpec {
    /// Center.
    pub at: Point,
    /// Solid core radius.
    pub core_r: f64,
    /// Ring inner radius.
    pub ring_in: f64,
    /// Ring outer radius.
    pub ring_out: f64,
    /// Radius of a background-colored pupil inside the core (0 = none).
    pub pupil_r: f64,
}

/// A data or header dot.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Dot {
    /// Axial column (0 for header dots).
    pub q: i32,
    /// Axial row (0 for header dots).
    pub r: i32,
    /// Position.
    pub at: Point,
    /// Hue index `0..4`.
    pub color: u8,
    /// 0 small, 1 large.
    pub size: u8,
}

impl Dot {
    /// Rest radius of the dot (before any animation).
    pub fn radius(&self) -> f64 {
        if self.size == 0 { dims::DOT_R_SMALL } else { dims::DOT_R_LARGE }
    }
}

/// A particle speck.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ParticleSpec {
    /// Encoded sector `0..16` on this particle's arc.
    pub sector: u8,
    /// Rest angle in radians (sector center). Animation may wobble it by less than
    /// a third of a sector.
    pub angle: f64,
}

/// Everything a painter needs to draw one data frame.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FrameSpec {
    /// Frame index within the session.
    pub index: u32,
    /// The decoded header this frame carries.
    pub header: Header,
    /// Profile (also in the header; repeated for painters).
    pub profile: Profile,
    /// Frame half-side `R` in pitch units. The quiet zone lies outside `[-R, R]²`.
    pub frame_half_side: f64,
    /// Particle band.
    pub particle_band: ParticleBand,
    /// Three hero anchors (top, lower right, lower left) then the center anchor.
    pub anchors: Vec<AnchorSpec>,
    /// 48 calibration swatches.
    pub swatches: Vec<Swatch>,
    /// 48 header dots.
    pub header_dots: Vec<Dot>,
    /// Payload dots in symbol order.
    pub dots: Vec<Dot>,
    /// Six particles.
    pub particles: Vec<ParticleSpec>,
}

impl FrameSpec {
    /// Assemble a frame from already-encoded symbols.
    ///
    /// `header_symbols` are the 48 header dot symbols, `body_symbols` one symbol per dot
    /// of `order`, `particle_sectors` the six encoded sectors.
    pub fn assemble(
        index: u32,
        header: Header,
        header_symbols: &[u8],
        order: &DotOrder,
        body_symbols: &[u8],
        particle_sectors: [u8; dims::PARTICLE_COUNT],
    ) -> FrameSpec {
        let profile = order.profile();
        assert_eq!(header_symbols.len(), layout::HEADER_DOTS);
        assert_eq!(body_symbols.len(), order.len());
        let band = order.particle_band();

        let mut anchors: Vec<AnchorSpec> = layout::hero_anchors(profile)
            .iter()
            .enumerate()
            .map(|(i, &at)| AnchorSpec {
                at,
                core_r: dims::HERO_CORE_R,
                ring_in: dims::HERO_RING_IN,
                ring_out: dims::HERO_RING_OUT,
                pupil_r: if i == 0 { dims::HERO_PUPIL_R } else { 0.0 },
            })
            .collect();
        anchors.push(AnchorSpec {
            at: layout::center_anchor(),
            core_r: dims::CENTER_CORE_R,
            ring_in: dims::CENTER_RING_IN,
            ring_out: dims::CENTER_RING_OUT,
            pupil_r: 0.0,
        });

        let header_dots = header_symbols
            .iter()
            .enumerate()
            .map(|(j, &s)| {
                let s = Sym3::new(s);
                Dot { q: 0, r: 0, at: layout::header_dot(profile, j), color: s.color(), size: s.size() }
            })
            .collect();

        let dots = order
            .points()
            .iter()
            .zip(body_symbols)
            .map(|(p, &s)| {
                let s = Sym3::new(s);
                Dot { q: p.q, r: p.r, at: p.at, color: s.color(), size: s.size() }
            })
            .collect();

        let particles = particle_sectors
            .iter()
            .enumerate()
            .map(|(k, &sector)| ParticleSpec { sector, angle: layout::particle_angle(k, sector) })
            .collect();

        FrameSpec {
            index,
            header,
            profile,
            frame_half_side: band.frame_half_side(),
            particle_band: band,
            anchors,
            swatches: layout::swatches(profile),
            header_dots,
            dots,
            particles,
        }
    }

    /// Body symbols in dot order (what a perfect decoder would read).
    pub fn body_symbols(&self) -> Vec<Sym3> {
        self.dots.iter().map(|d| Sym3::from_parts(d.color, d.size)).collect()
    }

    /// Header symbols in header-dot order.
    pub fn header_symbols(&self) -> Vec<Sym3> {
        self.header_dots.iter().map(|d| Sym3::from_parts(d.color, d.size)).collect()
    }

    /// Header followed by body symbols: the input [`crate::Receiver::feed`] expects.
    pub fn all_symbols(&self) -> Vec<Sym3> {
        let mut v = self.header_symbols();
        v.extend(self.body_symbols());
        v
    }
}
