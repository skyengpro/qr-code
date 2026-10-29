//! Planar homographies and the small amount of linear algebra needed to fit them.

/// A 3 × 3 projective map, row-major, acting on column vectors `(x, y, 1)`.
///
/// In this crate it maps *frame coordinates* (pitch units, origin at the frame center, y
/// down — see `particlecode_core::layout`) to *image pixels* (pixel centers at integers).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Homography(pub [[f64; 3]; 3]);

impl Homography {
    /// The identity map.
    pub const IDENTITY: Homography =
        Homography([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);

    /// Map a point. `None` when it lands at (or behind) infinity.
    #[inline]
    pub fn apply(&self, p: (f64, f64)) -> Option<(f64, f64)> {
        let m = &self.0;
        let w = m[2][0] * p.0 + m[2][1] * p.1 + m[2][2];
        if w.abs() < 1e-12 || !w.is_finite() {
            return None;
        }
        let x = (m[0][0] * p.0 + m[0][1] * p.1 + m[0][2]) / w;
        let y = (m[1][0] * p.0 + m[1][1] * p.1 + m[1][2]) / w;
        (x.is_finite() && y.is_finite()).then_some((x, y))
    }

    /// Matrix product `self · other` (apply `other` first).
    #[must_use]
    pub fn then_after(&self, other: &Homography) -> Homography {
        let mut r = [[0.0; 3]; 3];
        for (i, row) in r.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                *cell = (0..3).map(|k| self.0[i][k] * other.0[k][j]).sum();
            }
        }
        Homography(r)
    }

    /// Inverse map, `None` if singular.
    pub fn inverse(&self) -> Option<Homography> {
        let m = &self.0;
        let c00 = m[1][1] * m[2][2] - m[1][2] * m[2][1];
        let c01 = m[1][2] * m[2][0] - m[1][0] * m[2][2];
        let c02 = m[1][0] * m[2][1] - m[1][1] * m[2][0];
        let det = m[0][0] * c00 + m[0][1] * c01 + m[0][2] * c02;
        if det.abs() < 1e-14 || !det.is_finite() {
            return None;
        }
        let inv = 1.0 / det;
        let r = [
            [
                c00 * inv,
                (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv,
                (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv,
            ],
            [
                c01 * inv,
                (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv,
                (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv,
            ],
            [
                c02 * inv,
                (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv,
                (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv,
            ],
        ];
        Some(Homography(r))
    }

    /// Jacobian of the map at `p`: `[[du/dx, du/dy], [dv/dx, dv/dy]]`.
    pub fn jacobian(&self, p: (f64, f64)) -> Option<[[f64; 2]; 2]> {
        let m = &self.0;
        let w = m[2][0] * p.0 + m[2][1] * p.1 + m[2][2];
        if w.abs() < 1e-12 {
            return None;
        }
        let (u, v) = self.apply(p)?;
        Some([
            [(m[0][0] - u * m[2][0]) / w, (m[0][1] - u * m[2][1]) / w],
            [(m[1][0] - v * m[2][0]) / w, (m[1][1] - v * m[2][1]) / w],
        ])
    }

    /// Local area scale `|det J|` at `p`: image pixels squared per pitch squared.
    pub fn area_scale(&self, p: (f64, f64)) -> Option<f64> {
        let j = self.jacobian(p)?;
        let d = (j[0][0] * j[1][1] - j[0][1] * j[1][0]).abs();
        (d.is_finite() && d > 1e-12).then_some(d)
    }

    /// Local linear scale (pixels per pitch) at `p`: `sqrt(|det J|)`.
    pub fn scale_at(&self, p: (f64, f64)) -> Option<f64> {
        self.area_scale(p).map(f64::sqrt)
    }

    /// Exact affine map through three point pairs (`None` if the sources are collinear).
    pub fn affine_from_3(src: &[(f64, f64); 3], dst: &[(f64, f64); 3]) -> Option<Homography> {
        fit_affine(src, dst)
    }
}

/// Least-squares affine fit (`n ≥ 3`). Exact for three non-collinear points.
pub(crate) fn fit_affine(src: &[(f64, f64)], dst: &[(f64, f64)]) -> Option<Homography> {
    let n = src.len().min(dst.len());
    if n < 3 {
        return None;
    }
    // Normal equations for [x y 1] · [a b c]^T = target, shared matrix for u and v.
    let mut ata = [[0.0f64; 3]; 3];
    let mut atu = [0.0f64; 3];
    let mut atv = [0.0f64; 3];
    for (s, d) in src.iter().zip(dst).take(n) {
        let row = [s.0, s.1, 1.0];
        for i in 0..3 {
            for j in 0..3 {
                ata[i][j] += row[i] * row[j];
            }
            atu[i] += row[i] * d.0;
            atv[i] += row[i] * d.1;
        }
    }
    let mut a = [[0.0; 4]; 3];
    let mut b = [[0.0; 4]; 3];
    for i in 0..3 {
        a[i][..3].copy_from_slice(&ata[i]);
        b[i][..3].copy_from_slice(&ata[i]);
        a[i][3] = atu[i];
        b[i][3] = atv[i];
    }
    let u = solve(&mut a)?;
    let v = solve(&mut b)?;
    Some(Homography([
        [u[0], u[1], u[2]],
        [v[0], v[1], v[2]],
        [0.0, 0.0, 1.0],
    ]))
}

/// Least-squares projective fit with Hartley normalization and two re-weighting passes
/// that turn the algebraic error into (approximately) the image-space error.
///
/// Needs at least four pairs, not all collinear. `None` on degenerate input.
pub(crate) fn fit_homography(src: &[(f64, f64)], dst: &[(f64, f64)]) -> Option<Homography> {
    let n = src.len().min(dst.len());
    if n < 4 {
        return None;
    }
    let (ts, ss) = normalization(&src[..n])?;
    let (td, sd) = normalization(&dst[..n])?;
    let s: Vec<(f64, f64)> = src[..n]
        .iter()
        .map(|p| ((p.0 - ts.0) * ss, (p.1 - ts.1) * ss))
        .collect();
    let d: Vec<(f64, f64)> = dst[..n]
        .iter()
        .map(|p| ((p.0 - td.0) * sd, (p.1 - td.1) * sd))
        .collect();

    let mut h = [0.0f64; 8];
    let mut weights = vec![1.0f64; n];
    for pass in 0..3 {
        let mut ata = [[0.0f64; 9]; 8];
        for ((p, q), &wt) in s.iter().zip(&d).zip(&weights) {
            let rows = [
                ([p.0, p.1, 1.0, 0.0, 0.0, 0.0, -q.0 * p.0, -q.0 * p.1], q.0),
                ([0.0, 0.0, 0.0, p.0, p.1, 1.0, -q.1 * p.0, -q.1 * p.1], q.1),
            ];
            for (row, rhs) in rows {
                for i in 0..8 {
                    for j in 0..8 {
                        ata[i][j] += wt * wt * row[i] * row[j];
                    }
                    ata[i][8] += wt * wt * row[i] * rhs;
                }
            }
        }
        h = solve(&mut ata)?;
        if pass < 2 {
            for ((p, wt), _) in s.iter().zip(weights.iter_mut()).zip(&d) {
                let denom = h[6] * p.0 + h[7] * p.1 + 1.0;
                *wt = if denom.abs() > 1e-6 { 1.0 / denom } else { 1.0 };
            }
        }
    }
    let hn = Homography([[h[0], h[1], h[2]], [h[3], h[4], h[5]], [h[6], h[7], 1.0]]);
    // Undo the normalization: H = Td^-1 · Hn · Ts.
    let t_src = Homography([
        [ss, 0.0, -ss * ts.0],
        [0.0, ss, -ss * ts.1],
        [0.0, 0.0, 1.0],
    ]);
    let t_dst_inv = Homography([
        [1.0 / sd, 0.0, td.0],
        [0.0, 1.0 / sd, td.1],
        [0.0, 0.0, 1.0],
    ]);
    let full = t_dst_inv.then_after(&hn).then_after(&t_src);
    let scale = full.0[2][2];
    if scale.abs() < 1e-12 || !scale.is_finite() {
        return None;
    }
    let mut m = full.0;
    for row in &mut m {
        for c in row {
            *c /= scale;
        }
    }
    m.iter()
        .flatten()
        .all(|c| c.is_finite())
        .then_some(Homography(m))
}

/// Hartley normalization: centroid and scale so the mean distance from it is `sqrt(2)`.
fn normalization(pts: &[(f64, f64)]) -> Option<((f64, f64), f64)> {
    let n = pts.len() as f64;
    let cx = pts.iter().map(|p| p.0).sum::<f64>() / n;
    let cy = pts.iter().map(|p| p.1).sum::<f64>() / n;
    let mean = pts.iter().map(|p| (p.0 - cx).hypot(p.1 - cy)).sum::<f64>() / n;
    (mean > 1e-9 && mean.is_finite()).then_some(((cx, cy), std::f64::consts::SQRT_2 / mean))
}

/// Gaussian elimination with partial pivoting on an augmented `N × (N+1)` matrix.
fn solve<const N: usize, const M: usize>(a: &mut [[f64; M]; N]) -> Option<[f64; N]> {
    debug_assert_eq!(M, N + 1);
    for col in 0..N {
        let pivot = (col..N).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() < 1e-12 || !a[pivot][col].is_finite() {
            return None;
        }
        a.swap(col, pivot);
        for row in col + 1..N {
            let f = a[row][col] / a[col][col];
            if f != 0.0 {
                let pivot_row = a[col];
                for (cell, &p) in a[row].iter_mut().zip(&pivot_row).skip(col) {
                    *cell -= f * p;
                }
            }
        }
    }
    let mut x = [0.0; N];
    for row in (0..N).rev() {
        let tail: f64 = (row + 1..N).map(|k| a[row][k] * x[k]).sum();
        x[row] = (a[row][N] - tail) / a[row][row];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn truth() -> Homography {
        Homography([
            [6.5, 1.2, 330.0],
            [-0.9, 6.1, 310.0],
            [0.0004, -0.0003, 1.0],
        ])
    }

    #[test]
    fn inverse_roundtrip() {
        let h = truth();
        let inv = h.inverse().expect("invertible");
        let p = (12.3, -7.7);
        let back = inv.apply(h.apply(p).expect("finite")).expect("finite");
        assert!((back.0 - p.0).abs() < 1e-9 && (back.1 - p.1).abs() < 1e-9);
    }

    #[test]
    fn exact_fit_from_four_points() {
        let h = truth();
        let src = [(-30.0, 0.0), (15.0, 26.0), (15.0, -26.0), (0.0, 0.0)];
        let dst: Vec<_> = src.iter().map(|&p| h.apply(p).expect("finite")).collect();
        let fit = fit_homography(&src, &dst).expect("fit");
        for p in [(20.0, 20.0), (-35.0, 5.0), (3.0, -40.0)] {
            let a = h.apply(p).expect("finite");
            let b = fit.apply(p).expect("finite");
            assert!(
                (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6,
                "{a:?} {b:?}"
            );
        }
    }

    #[test]
    fn noisy_overdetermined_fit_is_close() {
        let h = truth();
        let mut src = Vec::new();
        let mut dst = Vec::new();
        let mut k = 1u32;
        for i in -5..=5 {
            for j in -5..=5 {
                let p = (f64::from(i) * 6.0, f64::from(j) * 6.0);
                k = k.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let nx = (f64::from(k >> 8) / f64::from(1u32 << 24) - 0.5) * 0.4;
                k = k.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let ny = (f64::from(k >> 8) / f64::from(1u32 << 24) - 0.5) * 0.4;
                let q = h.apply(p).expect("finite");
                src.push(p);
                dst.push((q.0 + nx, q.1 + ny));
            }
        }
        let fit = fit_homography(&src, &dst).expect("fit");
        let p = (10.0, -20.0);
        let (a, b) = (h.apply(p).expect("finite"), fit.apply(p).expect("finite"));
        assert!((a.0 - b.0).abs() < 0.15 && (a.1 - b.1).abs() < 0.15);
    }

    #[test]
    fn affine_three_points_and_degenerate() {
        let src = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)];
        let dst = [(10.0, 20.0), (12.0, 20.0), (10.0, 23.0)];
        let a = fit_affine(&src, &dst).expect("affine");
        assert_eq!(a.apply((1.0, 1.0)), Some((12.0, 23.0)));
        assert!(fit_affine(&[(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)], &dst).is_none());
        assert!(fit_homography(&[(0.0, 0.0); 5], &[(1.0, 1.0); 5]).is_none());
    }

    #[test]
    fn area_scale_of_pure_scale() {
        let h = Homography([[7.0, 0.0, 100.0], [0.0, 7.0, 100.0], [0.0, 0.0, 1.0]]);
        let a = h.area_scale((3.0, 4.0)).expect("scale");
        assert!((a - 49.0).abs() < 1e-9);
    }
}
