//! The 1000 × 1000 ink raster of a shape and its sampling at lattice points.

use particlecode_core::layout::{Lattice, to_design_box};
use particlecode_core::profile::Profile;
use particlecode_core::shapes::{ShapeTable, builtin_contains};
use tiny_skia::{FillRule, LineCap, LineJoin, Mask, Path, Stroke, Transform};

use crate::error::Error;
use crate::path::{DESIGN_SIZE, ViewBox, parse_path};
use crate::svg::extract_first_path;

const SIDE: usize = DESIGN_SIZE as usize;
/// Anti-aliased coverage at or above this counts as ink.
const INK_THRESHOLD: u8 = 128;

/// Ink / no ink per pixel of the 1000 × 1000 design box (y down). Pixel `(i, j)` covers the
/// design-box square `[i, i+1) × [j, j+1)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raster {
    ink: Vec<bool>,
}

impl Raster {
    /// Fill `path` (non-zero rule) or, with `stroke_width`, fill its stroked outline instead
    /// (round caps and joins; the width is in design units).
    pub fn from_path(path: &Path, stroke_width: Option<f32>) -> Result<Raster, Error> {
        let outline;
        let target = match stroke_width {
            Some(width) => {
                if !width.is_finite() || width <= 0.0 {
                    return Err(Error::BadOption(format!(
                        "stroke width {width} must be positive"
                    )));
                }
                let stroke = Stroke {
                    width,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    ..Stroke::default()
                };
                outline = path.stroke(&stroke, 1.0).ok_or(Error::EmptyOutline)?;
                &outline
            }
            None => path,
        };
        let mut mask = Mask::new(DESIGN_SIZE, DESIGN_SIZE).ok_or(Error::EmptyOutline)?;
        mask.fill_path(target, FillRule::Winding, true, Transform::identity());
        Ok(Raster {
            ink: mask.data().iter().map(|&a| a >= INK_THRESHOLD).collect(),
        })
    }

    /// Raster of a built-in shape, sampled at pixel centres.
    pub fn from_builtin(id: u8) -> Option<Raster> {
        builtin_contains(id, 500.0, 500.0)?;
        let mut ink = Vec::with_capacity(SIDE * SIDE);
        for j in 0..SIDE {
            for i in 0..SIDE {
                let inside = builtin_contains(id, i as f64 + 0.5, j as f64 + 0.5);
                ink.push(inside.unwrap_or(false));
            }
        }
        Some(Raster { ink })
    }

    /// Raster of a lattice mask: every pixel takes the value of the nearest lattice point,
    /// so each dot becomes a hexagonal cell. Used for custom shapes that only exist as
    /// bitmaps.
    pub fn from_mask(profile: Profile, mask: &[bool]) -> Raster {
        let lattice = Lattice::new(profile);
        let pts = lattice.points();
        let f = profile.field_half();
        // Index of (q, r) in the canonical order: rows are r ascending, q ascending.
        let r_min = pts.first().map_or(0, |p| p.r);
        let mut row_start = Vec::new();
        let mut row_q_min = Vec::new();
        let mut row_len = Vec::new();
        let mut k = 0;
        while k < pts.len() {
            let r = pts[k].r;
            let n = pts[k..].iter().take_while(|p| p.r == r).count();
            row_start.push(k);
            row_q_min.push(pts[k].q);
            row_len.push(n);
            k += n;
        }
        let mut ink = Vec::with_capacity(SIDE * SIDE);
        for j in 0..SIDE {
            for i in 0..SIDE {
                let x = ((i as f64 + 0.5) / 500.0 - 1.0) * f;
                let y = ((j as f64 + 0.5) / 500.0 - 1.0) * f;
                let (q, r) = nearest_axial(x, y);
                let row = r - r_min;
                let hit = usize::try_from(row)
                    .ok()
                    .filter(|&row| row < row_start.len())
                    .and_then(|row| {
                        let dq = usize::try_from(q - row_q_min[row]).ok()?;
                        (dq < row_len[row]).then(|| row_start[row] + dq)
                    });
                ink.push(hit.is_some_and(|k| mask[k]));
            }
        }
        Raster { ink }
    }

    /// Whether the pixel under design-box point `(x, y)` is ink (coordinates are clamped to
    /// the box, so points on the right/bottom edge use the last pixel).
    pub fn is_ink(&self, x: f64, y: f64) -> bool {
        let clamp = |v: f64| (v.floor().max(0.0) as usize).min(SIDE - 1);
        self.ink[clamp(y) * SIDE + clamp(x)]
    }

    /// Whether pixel `(i, j)` is ink; false outside the raster.
    pub(crate) fn at(&self, i: isize, j: isize) -> bool {
        let side = SIDE as isize;
        (0..side).contains(&i) && (0..side).contains(&j) && self.ink[(j * side + i) as usize]
    }

    /// Fraction of the design box that is ink.
    pub fn coverage(&self) -> f64 {
        self.ink.iter().filter(|&&b| b).count() as f64 / self.ink.len() as f64
    }

    /// Sample at every lattice point of the three profiles: `[S, M, L]` masks in
    /// [`Lattice`] order.
    pub fn sample_masks(&self) -> [Vec<bool>; 3] {
        Profile::ALL.map(|profile| {
            Lattice::new(profile)
                .points()
                .iter()
                .map(|p| {
                    let (x, y) = to_design_box(profile, p.at);
                    self.is_ink(x, y)
                })
                .collect()
        })
    }
}

/// Axial coordinates of the lattice point nearest to the frame point `(x, y)`.
fn nearest_axial(x: f64, y: f64) -> (i32, i32) {
    let r = y / (3f64.sqrt() / 2.0);
    let q = x - r / 2.0;
    // Cube rounding.
    let (cx, cz) = (q, r);
    let cy = -cx - cz;
    let (mut rx, ry, mut rz) = (cx.round(), cy.round(), cz.round());
    let (dx, dy, dz) = ((rx - cx).abs(), (ry - cy).abs(), (rz - cz).abs());
    if dx > dy && dx > dz {
        rx = -ry - rz;
    } else if dy <= dz {
        rz = -rx - ry;
    }
    (rx as i32, rz as i32)
}

/// A shape ready to be analysed, previewed or built: its raster (for the stroke-width
/// rule) and its masks over the three lattices.
#[derive(Debug, Clone)]
pub struct ShapeInput {
    /// Where the shape came from, for reports.
    pub label: String,
    /// Ink raster in the design box.
    pub raster: Raster,
    /// `[S, M, L]` masks in lattice order.
    pub masks: [Vec<bool>; 3],
    /// Warnings gathered while reading (e.g. ink clipped by the box).
    pub notes: Vec<String>,
}

impl ShapeInput {
    /// From SVG path data in the design box (or `view_box` units), optionally stroked.
    pub fn from_path_data(
        d: &str,
        view_box: &ViewBox,
        stroke_width: Option<f32>,
        label: &str,
    ) -> Result<ShapeInput, Error> {
        let path = parse_path(d, view_box)?;
        let mut notes = Vec::new();
        let b = path.bounds();
        let size = DESIGN_SIZE as f32;
        let margin = stroke_width.map_or(0.0, |w| w / 2.0);
        if b.left() - margin < -0.5
            || b.top() - margin < -0.5
            || b.right() + margin > size + 0.5
            || b.bottom() + margin > size + 0.5
        {
            notes
                .push("the path extends beyond the 1000 x 1000 box; ink outside is clipped".into());
        }
        let raster = Raster::from_path(&path, stroke_width)?;
        let masks = raster.sample_masks();
        Ok(ShapeInput {
            label: label.to_string(),
            raster,
            masks,
            notes,
        })
    }

    /// From the text of an `.svg` file (first `<path>`).
    pub fn from_svg(
        svg: &str,
        stroke_width: Option<f32>,
        label: &str,
    ) -> Result<ShapeInput, Error> {
        let p = extract_first_path(svg)?;
        ShapeInput::from_path_data(&p.d, &p.view_box, stroke_width, label)
    }

    /// From shape `id` of a table: built-ins use their analytic raster, custom shapes a
    /// hexagon-cell raster of the finest (L) mask.
    pub fn from_table(table: &ShapeTable, id: u8) -> Option<ShapeInput> {
        let name = table.name(id)?;
        let masks = Profile::ALL.map(|p| table.mask(id, p).unwrap_or_default());
        let raster =
            Raster::from_builtin(id).unwrap_or_else(|| Raster::from_mask(Profile::L, &masks[2]));
        Some(ShapeInput {
            label: format!("{name} (id {id})"),
            raster,
            masks,
            notes: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(lo: f32, hi: f32) -> Path {
        parse_path(
            &format!("M {lo} {lo} L {hi} {lo} L {hi} {hi} L {lo} {hi} Z"),
            &ViewBox::DESIGN,
        )
        .unwrap()
    }

    #[test]
    fn square_coverage_is_the_square_area() {
        let r = Raster::from_path(&square(250.0, 750.0), None).unwrap();
        assert!((r.coverage() - 0.25).abs() < 1e-3, "{}", r.coverage());
        assert!(r.is_ink(500.0, 500.0));
        assert!(!r.is_ink(100.0, 100.0));
        assert!(!r.is_ink(1000.0, 1000.0));
    }

    #[test]
    fn square_lattice_coverage_matches() {
        let r = Raster::from_path(&square(250.0, 750.0), None).unwrap();
        let masks = r.sample_masks();
        for (profile, mask) in Profile::ALL.iter().zip(&masks) {
            let frac = mask.iter().filter(|&&b| b).count() as f64 / mask.len() as f64;
            assert!((frac - 0.25).abs() < 0.03, "{profile:?}: {frac}");
        }
    }

    #[test]
    fn stroke_outlines_the_path() {
        // An open horizontal line, stroked 100 wide: a 100 x 800 bar (+ round caps).
        let line = parse_path("M 100 500 L 900 500", &ViewBox::DESIGN).unwrap();
        let r = Raster::from_path(&line, Some(100.0)).unwrap();
        assert!(r.is_ink(500.0, 460.0) && r.is_ink(500.0, 540.0));
        assert!(!r.is_ink(500.0, 430.0));
        assert!(Raster::from_path(&line, Some(0.0)).is_err());
    }

    #[test]
    fn mask_raster_roundtrips_a_disc() {
        let table = ShapeTable::builtin();
        let mask = table.mask(0, Profile::M).unwrap();
        let r = Raster::from_mask(Profile::M, &mask);
        assert!((r.coverage() - std::f64::consts::FRAC_PI_4).abs() < 0.02);
    }
}
