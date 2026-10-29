//! The non-letter built-in shapes, as inside-tests in the 1000 × 1000 design box.

/// Box center on both axes.
const C: f64 = 500.0;

/// Disc of radius 500 centered in the box (boundary included).
pub(super) fn disc(x: f64, y: f64) -> bool {
    libm::hypot(x - C, y - C) <= 500.0
}

/// The box with generously rounded corners.
pub(super) fn rounded_square(x: f64, y: f64) -> bool {
    const CORNER: f64 = 200.0;
    const FLAT: f64 = 500.0 - CORNER;
    let (dx, dy) = ((x - C).abs(), (y - C).abs());
    if dx > 500.0 || dy > 500.0 {
        return false;
    }
    if dx <= FLAT || dy <= FLAT {
        return true;
    }
    libm::hypot(dx - FLAT, dy - FLAT) <= CORNER
}

/// Heart: the implicit curve `(X² + Y² − 1)³ − X²Y³ ≤ 0`, scaled and centered so its
/// bounding box (about 2.28 × 2.24 units) fills most of the design box.
pub(super) fn heart(x: f64, y: f64) -> bool {
    const SCALE: f64 = 390.0;
    // Vertical middle of the curve's extent (−1 .. 1.236).
    const MID: f64 = 0.118;
    let hx = (x - C) / SCALE;
    let hy = (C - y) / SCALE + MID;
    let a = hx * hx + hy * hy - 1.0;
    a * a * a - hx * hx * hy * hy * hy <= 0.0
}

/// Regular hexagon with a vertex straight up and down, circumradius 495, so the three hero
/// anchors sit inside its vertices with their halos. (At 495 no profile has a tip that the
/// anchor keep-outs would cut into a one- or two-dot island.)
pub(super) fn hexagon(x: f64, y: f64) -> bool {
    const R: f64 = 495.0;
    const SQRT3: f64 = 1.732_050_807_568_877_2;
    let (dx, dy) = ((x - C).abs(), (y - C).abs());
    dx <= R * SQRT3 / 2.0 && dy + dx / SQRT3 <= R
}

/// Annulus from radius 290 to 500.
pub(super) fn ring(x: f64, y: f64) -> bool {
    let d = libm::hypot(x - C, y - C);
    (290.0..=500.0).contains(&d)
}
