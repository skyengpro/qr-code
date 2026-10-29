//! Frame geometry, in units of the dot pitch `p`, origin at the frame center, `y` pointing
//! down (screen convention). Angles are in radians, measured from +x towards +y, so −90°
//! is straight up.
//!
//! This module is the single source of truth for where everything sits. The encoder,
//! every painter and the vision decoder all derive positions from it.
//!
//! * The **field** is the square `[-F, F]²` with `F = N / 2`. Shape masks map their
//!   1000 × 1000 design box onto it.
//! * Three **hero anchors** sit on a circle of radius `0.84 F` (the spec's `0.67 R` with
//!   `R = 1.25 F`) at −90°, 30° and 150°. A small **center anchor** sits at the origin.
//! * Each hero anchor has a **halo**: 16 calibration swatches on a ring of radius 3,
//!   then 16 header dots on a ring of radius 4.
//! * **Lattice** points of the hex lattice (axial coordinates `q`, `r`; `x = q + r/2`,
//!   `y = r·√3/2`) inside the field, outside every keep-out, and inside the shape mask
//!   carry payload symbols in serpentine order.
//! * **Particles** orbit in a band outside all ink.

use alloc::vec::Vec;
use core::f64::consts::PI;

use crate::profile::Profile;

/// Geometric constants, all in pitch units unless stated otherwise.
pub mod dims {
    /// Hero anchor circle radius as a fraction of the field half-side `F`.
    pub const ANCHOR_RADIUS_FRAC: f64 = 0.84;
    /// Hero anchor angles in degrees (y down, so −90° is up).
    pub const HERO_ANGLES_DEG: [f64; 3] = [-90.0, 30.0, 150.0];

    /// Hero mark: solid core radius.
    pub const HERO_CORE_R: f64 = 0.70;
    /// Hero mark: ring inner radius.
    pub const HERO_RING_IN: f64 = 1.30;
    /// Hero mark: ring outer radius. Ring area / core area ≈ 6.
    pub const HERO_RING_OUT: f64 = 2.15;
    /// The −90° anchor's "extra inner dot": a background-colored pupil in its core.
    pub const HERO_PUPIL_R: f64 = 0.25;

    /// Center mark: solid core radius.
    pub const CENTER_CORE_R: f64 = 0.33;
    /// Center mark: ring inner radius.
    pub const CENTER_RING_IN: f64 = 0.58;
    /// Center mark: ring outer radius. Ring area / core area ≈ 6.
    pub const CENTER_RING_OUT: f64 = 1.00;

    /// Radius of the calibration swatch ring around each hero anchor.
    pub const SWATCH_RING_R: f64 = 3.0;
    /// Swatch dot radius: between small and large, so its area is the size reference.
    pub const SWATCH_DOT_R: f64 = 0.42;
    /// Radius of the header dot ring around each hero anchor.
    pub const HEADER_RING_R: f64 = 4.0;
    /// Swatches / header dots per halo.
    pub const HALO_SLOTS: usize = 16;

    /// Lattice points closer than this to a hero anchor are not used.
    /// (Spec says 4.6; 5.0 keeps ≥ 1 p between header dots and the nearest lattice dot.)
    pub const HERO_KEEPOUT: f64 = 5.0;
    /// Lattice points closer than this to the center anchor are not used.
    pub const CENTER_KEEPOUT: f64 = 1.5;

    /// Small data dot radius.
    pub const DOT_R_SMALL: f64 = 0.36;
    /// Large data dot radius (area ratio 1.8 to small).
    pub const DOT_R_LARGE: f64 = 0.48;
    /// Radius of the disc the decoder samples around each lattice point.
    pub const SAMPLE_R: f64 = 0.30;
    /// Maximum allowed drift of an animated dot from its lattice point.
    pub const MAX_DRIFT: f64 = 0.15;

    /// Number of particle specks.
    pub const PARTICLE_COUNT: usize = 6;
    /// Angular sectors per particle arc (4 bits).
    pub const PARTICLE_SECTORS: usize = 16;
    /// Particle speck radius.
    pub const PARTICLE_R: f64 = 0.42;
    /// Minimum particle band inner radius as a fraction of `F` (spec: 0.85 R = 1.0625 F).
    pub const PARTICLE_BAND_MIN_FRAC: f64 = 1.0625;
    /// Particle band width as a fraction of `F` (spec: 0.1 R = 0.125 F).
    pub const PARTICLE_BAND_WIDTH_FRAC: f64 = 0.125;
    /// Clearance between the outermost ink and the particle band.
    pub const PARTICLE_CLEARANCE: f64 = 1.5;
    /// The particle band's outer edge sits at this fraction of the frame half-side `R`.
    pub const PARTICLE_OUTER_OF_R: f64 = 0.95;

    /// Minimum quiet zone around the frame's bounding box.
    pub const QUIET_ZONE: f64 = 1.5;
}

/// A point in frame coordinates (pitch units, origin at center, y down).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Point {
    /// Horizontal, rightwards.
    pub x: f64,
    /// Vertical, downwards.
    pub y: f64,
}

impl Point {
    /// Construct a point.
    pub const fn new(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    /// Point at `radius` and `angle` (radians) around `self`.
    pub fn polar(self, radius: f64, angle: f64) -> Point {
        Point::new(self.x + radius * libm::cos(angle), self.y + radius * libm::sin(angle))
    }

    /// Euclidean distance.
    pub fn dist(self, other: Point) -> f64 {
        libm::hypot(self.x - other.x, self.y - other.y)
    }

    /// Distance from the origin.
    pub fn norm(self) -> f64 {
        libm::hypot(self.x, self.y)
    }
}

/// Radius of the hero anchor circle for a profile.
pub fn anchor_radius(profile: Profile) -> f64 {
    dims::ANCHOR_RADIUS_FRAC * profile.field_half()
}

/// Radial angle (radians) of hero anchor `i` (0 = top, 1 = lower right, 2 = lower left).
pub fn hero_angle(i: usize) -> f64 {
    dims::HERO_ANGLES_DEG[i] * PI / 180.0
}

/// Center of hero anchor `i` (0 = top at −90°, 1 = 30°, 2 = 150°).
pub fn hero_anchor(profile: Profile, i: usize) -> Point {
    Point::default().polar(anchor_radius(profile), hero_angle(i))
}

/// The three hero anchor centers.
pub fn hero_anchors(profile: Profile) -> [Point; 3] {
    [hero_anchor(profile, 0), hero_anchor(profile, 1), hero_anchor(profile, 2)]
}

/// Center of the center anchor (the origin, which is also a lattice point).
pub const fn center_anchor() -> Point {
    Point::new(0.0, 0.0)
}

/// What a calibration swatch shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SwatchColor {
    /// A data hue (`0..4`).
    Hue(u8),
    /// The white reference.
    White,
    /// The black reference.
    Black,
}

/// A calibration swatch.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Swatch {
    /// Center.
    pub at: Point,
    /// Color.
    pub color: SwatchColor,
    /// Which hero anchor's halo (0..3).
    pub anchor: u8,
    /// Slot on the ring (0..16); slot 0 points away from the frame center.
    pub slot: u8,
}

/// Slot of the white swatch on the top anchor's halo.
pub const WHITE_SLOT: usize = 0;
/// Slot of the black swatch on the top anchor's halo.
pub const BLACK_SLOT: usize = 8;

/// Swatch color for `(anchor, slot)`: hues cycle `slot % 4`; the top anchor swaps slot 0
/// for white and slot 8 for black.
pub fn swatch_color(anchor: usize, slot: usize) -> SwatchColor {
    match (anchor, slot) {
        (0, WHITE_SLOT) => SwatchColor::White,
        (0, BLACK_SLOT) => SwatchColor::Black,
        _ => SwatchColor::Hue((slot % 4) as u8),
    }
}

/// Swatch `slot` of hero anchor `anchor`. Slot 0 points radially outwards; slots advance
/// by 22.5° in the direction of increasing angle (clockwise on screen).
pub fn swatch(profile: Profile, anchor: usize, slot: usize) -> Swatch {
    let a = hero_anchor(profile, anchor);
    let angle = hero_angle(anchor) + slot as f64 * 2.0 * PI / dims::HALO_SLOTS as f64;
    Swatch {
        at: a.polar(dims::SWATCH_RING_R, angle),
        color: swatch_color(anchor, slot),
        anchor: anchor as u8,
        slot: slot as u8,
    }
}

/// All 48 swatches, anchor-major.
pub fn swatches(profile: Profile) -> Vec<Swatch> {
    (0..3)
        .flat_map(|a| (0..dims::HALO_SLOTS).map(move |s| swatch(profile, a, s)))
        .collect()
}

/// Number of header dots.
pub const HEADER_DOTS: usize = 3 * dims::HALO_SLOTS;

/// Position of header dot `j` (`0..48`): halo `j / 16`, slot `j % 16`, offset half a slot
/// from the swatches so the two rings interleave.
pub fn header_dot(profile: Profile, j: usize) -> Point {
    let anchor = j / dims::HALO_SLOTS;
    let slot = j % dims::HALO_SLOTS;
    let step = 2.0 * PI / dims::HALO_SLOTS as f64;
    let angle = hero_angle(anchor) + (slot as f64 + 0.5) * step;
    hero_anchor(profile, anchor).polar(dims::HEADER_RING_R, angle)
}

/// Axial hex coordinates to frame coordinates.
pub fn axial_to_point(q: i32, r: i32) -> Point {
    Point::new(f64::from(q) + f64::from(r) / 2.0, f64::from(r) * SQRT3_2)
}

const SQRT3_2: f64 = 0.866_025_403_784_438_6;

/// A lattice point of the field.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LatticePoint {
    /// Axial column.
    pub q: i32,
    /// Axial row (0 at the center row, negative above).
    pub r: i32,
    /// Frame position.
    pub at: Point,
}

/// Every hex lattice point inside the field square `[-F, F]²`, in canonical order: rows
/// top to bottom (`r` ascending), then left to right (`q` ascending). Shape masks are
/// bitmaps over this order.
#[derive(Debug, Clone)]
pub struct Lattice {
    profile: Profile,
    points: Vec<LatticePoint>,
}

impl Lattice {
    /// Build the lattice for a profile.
    pub fn new(profile: Profile) -> Lattice {
        let f = profile.field_half();
        let r_max = libm::floor(f / SQRT3_2 + 1e-9) as i32;
        let mut points = Vec::new();
        for r in -r_max..=r_max {
            // x = q + r/2 ∈ [-F, F]
            let q_min = libm::ceil(-f - f64::from(r) / 2.0 - 1e-9) as i32;
            let q_max = libm::floor(f - f64::from(r) / 2.0 + 1e-9) as i32;
            for q in q_min..=q_max {
                points.push(LatticePoint { q, r, at: axial_to_point(q, r) });
            }
        }
        Lattice { profile, points }
    }

    /// Profile this lattice belongs to.
    pub fn profile(&self) -> Profile {
        self.profile
    }

    /// Points in canonical order.
    pub fn points(&self) -> &[LatticePoint] {
        &self.points
    }

    /// Number of points.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Whether the lattice is empty (never, for real profiles).
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

/// Map a frame point to the shape design box (`0..1000` on both axes, y down), which
/// covers the field square `[-F, F]²`.
pub fn to_design_box(profile: Profile, p: Point) -> (f64, f64) {
    let f = profile.field_half();
    ((p.x / f + 1.0) * 500.0, (p.y / f + 1.0) * 500.0)
}

/// Map a design-box point (`0..1000`) back to frame coordinates.
pub fn from_design_box(profile: Profile, x: f64, y: f64) -> Point {
    let f = profile.field_half();
    Point::new((x / 500.0 - 1.0) * f, (y / 500.0 - 1.0) * f)
}

/// Whether a frame point lies inside an anchor keep-out (hero or center).
pub fn in_keepout(profile: Profile, p: Point) -> bool {
    p.norm() < dims::CENTER_KEEPOUT
        || hero_anchors(profile).iter().any(|a| a.dist(p) < dims::HERO_KEEPOUT)
}

/// The payload-carrying dots of a (profile, mask) pair, in serpentine symbol order.
///
/// Symbol `i` of the body goes to `DotOrder::points()[i]`. Rows run top to bottom;
/// even rows (counted from the top row of the lattice) run left to right, odd rows right
/// to left. Keep-outs and points outside the mask are skipped.
#[derive(Debug, Clone)]
pub struct DotOrder {
    profile: Profile,
    points: Vec<LatticePoint>,
}

impl DotOrder {
    /// Build the order. `mask[k]` says whether `lattice.points()[k]` has ink; it must have
    /// the same length as the lattice.
    pub fn new(lattice: &Lattice, mask: &[bool]) -> DotOrder {
        assert_eq!(mask.len(), lattice.len(), "mask length must match the lattice");
        let profile = lattice.profile();
        let pts = lattice.points();
        let mut points = Vec::with_capacity(pts.len());
        let r_min = pts.first().map_or(0, |p| p.r);
        let mut start = 0;
        while start < pts.len() {
            let r = pts[start].r;
            let end = start + pts[start..].iter().take_while(|p| p.r == r).count();
            let keep = |k: &usize| mask[*k] && !in_keepout(profile, pts[*k].at);
            if (r - r_min) % 2 == 0 {
                points.extend((start..end).filter(keep).map(|k| pts[k]));
            } else {
                points.extend((start..end).rev().filter(keep).map(|k| pts[k]));
            }
            start = end;
        }
        DotOrder { profile, points }
    }

    /// Profile.
    pub fn profile(&self) -> Profile {
        self.profile
    }

    /// Dots in symbol order.
    pub fn points(&self) -> &[LatticePoint] {
        &self.points
    }

    /// Number of payload dots.
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Whether there are no payload dots.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Outermost extent of all ink (data dots, halos) from the frame center.
    pub fn ink_extent(&self) -> f64 {
        let dots = self.points.iter().map(|p| p.at.norm() + dims::DOT_R_LARGE).fold(0.0, f64::max);
        let halo = anchor_radius(self.profile) + dims::HEADER_RING_R + dims::DOT_R_LARGE;
        dots.max(halo)
    }

    /// The particle band for this mask.
    pub fn particle_band(&self) -> ParticleBand {
        ParticleBand::for_extent(self.profile, self.ink_extent())
    }
}

/// The annulus particles orbit in.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ParticleBand {
    /// Inner radius.
    pub inner: f64,
    /// Outer radius.
    pub outer: f64,
}

impl ParticleBand {
    /// Band clear of all ink up to `ink_extent`.
    pub fn for_extent(profile: Profile, ink_extent: f64) -> ParticleBand {
        let f = profile.field_half();
        let inner =
            (dims::PARTICLE_BAND_MIN_FRAC * f).max(ink_extent + dims::PARTICLE_CLEARANCE);
        ParticleBand { inner, outer: inner + dims::PARTICLE_BAND_WIDTH_FRAC * f }
    }

    /// Middle radius of the band.
    pub fn mid(&self) -> f64 {
        0.5 * (self.inner + self.outer)
    }

    /// Frame half-side `R` (the band's outer edge is at 0.95 R).
    pub fn frame_half_side(&self) -> f64 {
        self.outer / dims::PARTICLE_OUTER_OF_R
    }
}

/// Start angle (radians) of particle `k`'s 60° arc. Arc 0 starts straight up.
pub fn particle_arc_start(k: usize) -> f64 {
    -PI / 2.0 + k as f64 * 2.0 * PI / dims::PARTICLE_COUNT as f64
}

/// Angular width of one particle sector (3.75°).
pub fn particle_sector_width() -> f64 {
    2.0 * PI / (dims::PARTICLE_COUNT * dims::PARTICLE_SECTORS) as f64
}

/// Center angle of `sector` (`0..16`) on particle `k`'s arc.
pub fn particle_angle(k: usize, sector: u8) -> f64 {
    particle_arc_start(k) + (f64::from(sector) + 0.5) * particle_sector_width()
}

/// Sector (`0..16`) of an angle on particle `k`'s arc, or `None` if outside the arc.
pub fn particle_sector(k: usize, angle: f64) -> Option<u8> {
    let rel = (angle - particle_arc_start(k)).rem_euclid(2.0 * PI);
    let s = libm::floor(rel / particle_sector_width());
    (s >= 0.0 && s < dims::PARTICLE_SECTORS as f64).then_some(s as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disc_capacity_matches_spec_ballpark() {
        for (profile, lo, hi) in
            [(Profile::S, 1700, 2100), (Profile::M, 4200, 4700), (Profile::L, 7900, 8400)]
        {
            let lattice = Lattice::new(profile);
            let f = profile.field_half();
            let mask: Vec<bool> = lattice.points().iter().map(|p| p.at.norm() <= f).collect();
            let order = DotOrder::new(&lattice, &mask);
            assert!((lo..hi).contains(&order.len()), "{profile:?}: {}", order.len());
        }
    }

    #[test]
    fn halo_does_not_touch_lattice_dots() {
        for profile in Profile::ALL {
            let lattice = Lattice::new(profile);
            let mask = alloc::vec![true; lattice.len()];
            let order = DotOrder::new(&lattice, &mask);
            for j in 0..HEADER_DOTS {
                let h = header_dot(profile, j);
                for p in order.points() {
                    assert!(h.dist(p.at) >= 2.0 * dims::DOT_R_LARGE, "{profile:?} header {j}");
                }
            }
        }
    }

    #[test]
    fn serpentine_is_a_permutation_of_kept_points() {
        let lattice = Lattice::new(Profile::S);
        let mask = alloc::vec![true; lattice.len()];
        let order = DotOrder::new(&lattice, &mask);
        let kept = lattice.points().iter().filter(|p| !in_keepout(Profile::S, p.at)).count();
        assert_eq!(order.len(), kept);
        // Consecutive dots within a row are neighbours.
        let pts = order.points();
        for w in pts.windows(2) {
            if w[0].r == w[1].r {
                assert!(w[0].at.dist(w[1].at) >= 1.0 - 1e-9);
            }
        }
    }

    #[test]
    fn particle_sectors_roundtrip() {
        for k in 0..dims::PARTICLE_COUNT {
            for s in 0..dims::PARTICLE_SECTORS as u8 {
                assert_eq!(particle_sector(k, particle_angle(k, s)), Some(s));
            }
        }
    }

    #[test]
    fn disc_band_matches_spec_proportions() {
        let lattice = Lattice::new(Profile::M);
        let f = Profile::M.field_half();
        let mask: Vec<bool> = lattice.points().iter().map(|p| p.at.norm() <= f).collect();
        let band = DotOrder::new(&lattice, &mask).particle_band();
        let r = band.frame_half_side();
        assert!((band.inner / r - 0.85).abs() < 0.03, "{}", band.inner / r);
    }
}
