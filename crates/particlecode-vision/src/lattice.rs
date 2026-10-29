//! Lattice-phase measurement and neighbour lookup on the hex lattice.
//!
//! **Why phase.** Under blur neighbouring dots overlap so much that the ink centroid of one
//! dot is pulled towards wherever the search window starts. The lattice itself, however,
//! is periodic: the Fourier coefficients of the ink image at the three reciprocal lattice
//! vectors have a phase that depends only on how far the lattice is displaced from the
//! prediction, whatever the dot sizes and colours are — every dot sits on a lattice point
//! and contributes a positive real amount at the fundamental. Blur only shrinks the
//! amplitude. A handful of such measurements spread over the field pin down the homography
//! far better than per-dot centroids.

use particlecode_core::layout::LatticePoint;

use crate::homography::Homography;
use crate::image::PlaneRef;

/// `1 / sqrt(3)`.
const INV_SQRT3: f64 = 0.577_350_269_189_625_8;

/// Reciprocal lattice vectors of the hex lattice with basis `(1, 0)` and `(1/2, sqrt(3)/2)`:
/// `b1 · a1 = 1, b1 · a2 = 0` and so on; `b3 = b1 + b2`.
const RECIPROCAL: [(f64, f64); 3] = [(1.0, -INV_SQRT3), (0.0, 2.0 * INV_SQRT3), (1.0, INV_SQRT3)];

/// Window radius (pitch units) of a phase measurement.
const WINDOW_R: f64 = 2.4;

/// Displacement of the true lattice from the prediction `h` around frame point `c`,
/// in frame (pitch) units: the dots near `c` sit at `p + d` where `p` is where `h` expects
/// lattice points. Valid for `|d|` below about 0.4 pitch. `None` if the window is empty,
/// leaves the image, or the three phases disagree (foreign ink in the window).
pub(crate) fn lattice_shift(
    luma: PlaneRef<'_>,
    h: &Homography,
    bg: f64,
    c: (f64, f64),
) -> Option<(f64, f64)> {
    let centre = h.apply(c)?;
    let j = h.jacobian(c)?;
    let det = j[0][0] * j[1][1] - j[0][1] * j[1][0];
    if det.abs() < 1e-9 {
        return None;
    }
    // Image offset -> frame offset (inverse of the local linearisation).
    let inv = [
        [j[1][1] / det, -j[0][1] / det],
        [-j[1][0] / det, j[0][0] / det],
    ];
    let scale = det.abs().sqrt();
    let reach = WINDOW_R * scale * 1.35 + 2.0;
    if !luma.contains(centre.0, centre.1, reach + 1.0) {
        return None;
    }
    let dead = 0.08 * bg;
    let (cx, cy) = (centre.0.round() as i64, centre.1.round() as i64);
    let (fx, fy) = (centre.0 - cx as f64, centre.1 - cy as f64);
    let half = reach.ceil() as i64;
    let step = ((scale / 5.0).floor() as i64).max(1);
    let two_pi = std::f64::consts::TAU;
    // Phase of reciprocal vector m at pixel offset (ix, iy) from (cx, cy) is linear:
    // psi = psi0 + alpha * ix + beta * iy. Walk along a row by complex rotation, no trig.
    let mut psi0 = [0.0f64; 3];
    let mut alpha = [0.0f64; 3];
    let mut beta = [0.0f64; 3];
    for (m, b) in RECIPROCAL.iter().enumerate() {
        // u = inv * (i - f)
        let (u_ix, u_iy) = ((inv[0][0], inv[1][0]), (inv[0][1], inv[1][1]));
        let u0 = (
            -(inv[0][0] * fx + inv[0][1] * fy),
            -(inv[1][0] * fx + inv[1][1] * fy),
        );
        psi0[m] = two_pi * (b.0 * (c.0 + u0.0) + b.1 * (c.1 + u0.1));
        alpha[m] = two_pi * (b.0 * u_ix.0 + b.1 * u_ix.1);
        beta[m] = two_pi * (b.0 * u_iy.0 + b.1 * u_iy.1);
    }
    let rot: [(f64, f64); 3] = std::array::from_fn(|m| {
        let (sn, cs) = (alpha[m] * step as f64).sin_cos();
        (cs, -sn) // e^{-i alpha step}
    });
    let r2_max = WINDOW_R * WINDOW_R;
    let mut f = [(0.0f64, 0.0f64); 3];
    let mut total = 0.0;
    let mut iy = -half;
    while iy <= half {
        let row = luma.row((cy + iy) as usize);
        // Phasor at the start of the row, e^{-i psi}.
        let ix0 = -half;
        let mut z: [(f64, f64); 3] = std::array::from_fn(|m| {
            let (sn, cs) = (psi0[m] + alpha[m] * ix0 as f64 + beta[m] * iy as f64).sin_cos();
            (cs, -sn)
        });
        let mut ix = ix0;
        while ix <= half {
            let (dx, dy) = (ix as f64 - fx, iy as f64 - fy);
            let u = (
                inv[0][0] * dx + inv[0][1] * dy,
                inv[1][0] * dx + inv[1][1] * dy,
            );
            let r2 = u.0 * u.0 + u.1 * u.1;
            if r2 < r2_max {
                let ink = (bg - f64::from(row[(cx + ix) as usize]) - dead).max(0.0);
                if ink > 0.0 {
                    let t = 1.0 - r2 / r2_max;
                    let w = ink * t * t;
                    total += w;
                    for m in 0..3 {
                        f[m].0 += w * z[m].0;
                        f[m].1 += w * z[m].1;
                    }
                }
            }
            for m in 0..3 {
                z[m] = (
                    z[m].0 * rot[m].0 - z[m].1 * rot[m].1,
                    z[m].0 * rot[m].1 + z[m].1 * rot[m].0,
                );
            }
            ix += step;
        }
        iy += step;
    }
    if total < 1e-6 {
        return None;
    }
    // Fractional lattice coordinate of each phase, in (-1/2, 1/2].
    let mut theta = [0.0; 3];
    for m in 0..3 {
        let amp = f[m].0.hypot(f[m].1) / total;
        if amp < 0.002 {
            return None;
        }
        let t = -f[m].1.atan2(f[m].0) / two_pi;
        theta[m] = t - t.round();
    }
    // b3 = b1 + b2, so the three fractional phases must close.
    let closure = theta[0] + theta[1] - theta[2];
    let closure = closure - closure.round();
    if closure.abs() > 0.12 {
        return None;
    }
    // Least squares: sum(b b^T) = 2 I, hence d = (1/2) sum(theta_m b_m).
    let d = (
        0.5 * (theta[0] * RECIPROCAL[0].0
            + theta[1] * RECIPROCAL[1].0
            + theta[2] * RECIPROCAL[2].0),
        0.5 * (theta[0] * RECIPROCAL[0].1
            + theta[1] * RECIPROCAL[1].1
            + theta[2] * RECIPROCAL[2].1),
    );
    (d.0.hypot(d.1) < 0.6).then_some(d)
}

/// Neighbour lookup on the axial hex lattice of a dot order.
pub(crate) struct Neighbours {
    qmin: i32,
    qmax: i32,
    rmin: i32,
    rmax: i32,
    width: usize,
    grid: Vec<u32>,
}

impl Neighbours {
    /// Axial offsets of the six neighbours.
    const OFFSETS: [(i32, i32); 6] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, -1), (-1, 1)];

    pub(crate) fn new(pts: &[LatticePoint]) -> Neighbours {
        let (qmin, qmax) = pts
            .iter()
            .fold((i32::MAX, i32::MIN), |a, p| (a.0.min(p.q), a.1.max(p.q)));
        let (rmin, rmax) = pts
            .iter()
            .fold((i32::MAX, i32::MIN), |a, p| (a.0.min(p.r), a.1.max(p.r)));
        let width = (i64::from(qmax) - i64::from(qmin) + 1).max(0) as usize;
        let height = (i64::from(rmax) - i64::from(rmin) + 1).max(0) as usize;
        // A real order spans a few hundred cells per side; anything absurd gets no lookup.
        if width.saturating_mul(height) > 4_000_000 {
            return Neighbours {
                qmin: 0,
                qmax: -1,
                rmin: 0,
                rmax: -1,
                width: 0,
                grid: Vec::new(),
            };
        }
        let mut grid = vec![u32::MAX; width * height];
        for (i, p) in pts.iter().enumerate() {
            grid[(p.r - rmin) as usize * width + (p.q - qmin) as usize] = i as u32;
        }
        Neighbours {
            qmin,
            qmax,
            rmin,
            rmax,
            width,
            grid,
        }
    }

    /// Indices of the six lattice neighbours that exist in the order.
    pub(crate) fn of(&self, p: &LatticePoint) -> [Option<u32>; 6] {
        Self::OFFSETS.map(|(dq, dr)| {
            let (q, r) = (p.q + dq, p.r + dr);
            if q < self.qmin || q > self.qmax || r < self.rmin || r > self.rmax {
                return None;
            }
            let j = self.grid[(r - self.rmin) as usize * self.width + (q - self.qmin) as usize];
            (j != u32::MAX).then_some(j)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Plane;
    use particlecode_core::layout::axial_to_point;

    /// Render a hex lattice of dots (radius `r` pitch) with a given pixel scale and shift.
    fn lattice_image(scale: f64, shift: (f64, f64), blur_extra: f64) -> (Plane, Homography) {
        let size = 200usize;
        let mut data = vec![244.0f64; size * size];
        let origin = (100.0, 100.0);
        for q in -12..=12 {
            for r in -12..=12 {
                let p = axial_to_point(q, r);
                let (cx, cy) = (
                    origin.0 + (p.x + shift.0) * scale,
                    origin.1 + (p.y + shift.1) * scale,
                );
                let radius = if (q + 2 * r) % 3 == 0 { 0.48 } else { 0.36 } * scale;
                for y in (cy - radius - 3.0) as usize..(cy + radius + 3.0) as usize {
                    for x in (cx - radius - 3.0) as usize..(cx + radius + 3.0) as usize {
                        if x < size && y < size {
                            let d = (x as f64 - cx).hypot(y as f64 - cy);
                            let a = (radius - d + 0.5 + blur_extra).clamp(0.0, 1.0);
                            data[y * size + x] -= 130.0 * a;
                        }
                    }
                }
            }
        }
        let plane = Plane::new(
            size,
            size,
            data.iter()
                .map(|&v| v.round().clamp(0.0, 255.0) as u8)
                .collect(),
        );
        let h = Homography([
            [scale, 0.0, origin.0],
            [0.0, scale, origin.1],
            [0.0, 0.0, 1.0],
        ]);
        (plane, h)
    }

    #[test]
    fn recovers_a_sub_pitch_shift() {
        for shift in [(0.0, 0.0), (0.2, -0.1), (-0.3, 0.25), (0.05, 0.35)] {
            let (plane, h) = lattice_image(8.0, shift, 0.0);
            let view = plane.view().expect("valid");
            let d = lattice_shift(view, &h, 244.0, (2.0, 1.0)).expect("shift");
            assert!(
                (d.0 - shift.0).abs() < 0.03 && (d.1 - shift.1).abs() < 0.03,
                "{shift:?} -> {d:?}"
            );
        }
    }

    #[test]
    fn empty_window_gives_nothing() {
        let plane = Plane::filled(200, 200, 244);
        let h = Homography([[8.0, 0.0, 100.0], [0.0, 8.0, 100.0], [0.0, 0.0, 1.0]]);
        assert!(lattice_shift(plane.view().expect("valid"), &h, 244.0, (0.0, 0.0)).is_none());
    }

    #[test]
    fn neighbours_of_a_hexagon() {
        let pts: Vec<LatticePoint> = (-1..=1)
            .flat_map(|r| {
                (-1..=1).map(move |q| LatticePoint {
                    q,
                    r,
                    at: axial_to_point(q, r),
                })
            })
            .collect();
        let nb = Neighbours::new(&pts);
        let centre = pts.iter().find(|p| p.q == 0 && p.r == 0).expect("centre");
        assert_eq!(nb.of(centre).iter().filter(|n| n.is_some()).count(), 6);
        let corner = pts.iter().find(|p| p.q == -1 && p.r == -1).expect("corner");
        assert!(nb.of(corner).iter().filter(|n| n.is_some()).count() < 6);
    }
}
