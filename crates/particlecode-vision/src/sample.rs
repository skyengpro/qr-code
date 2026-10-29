//! Pixel-level measurements around a predicted dot position: ink centroid, mean chroma and
//! ink mass.
//!
//! "Ink" is how much darker than the background a pixel is, `bg − luma`. Every dot colour is
//! far from the background in luma (98–129 against 244) and blur only redistributes ink, so
//! ink mass is a blur-invariant soft area.

use crate::homography::Homography;
use crate::image::{PlaneRef, YuvRef};

/// Background estimate: luma and chroma of the empty paper.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Background {
    /// Paper luma at the frame center.
    pub(crate) luma: f64,
    pub(crate) cb: f64,
    pub(crate) cr: f64,
    /// Where the paper level varies across the picture (lighting gradient): luma per image
    /// pixel along x and y.
    pub(crate) gradient: (f64, f64),
    /// Image position that `luma` refers to.
    pub(crate) centre: (f64, f64),
}

impl Background {
    /// Paper luma at an image position, following the fitted plane.
    pub(crate) fn at(&self, x: f64, y: f64) -> f64 {
        self.luma + self.gradient.0 * (x - self.centre.0) + self.gradient.1 * (y - self.centre.1)
    }

    /// Relative luma variation across a span of `extent` pixels.
    pub(crate) fn variation(&self, extent: f64) -> f64 {
        self.gradient.0.hypot(self.gradient.1) * extent / self.luma.max(1.0)
    }

    /// The same background with the lighting gradient removed (after flattening).
    pub(crate) fn flattened(&self) -> Background {
        Background {
            gradient: (0.0, 0.0),
            ..*self
        }
    }
}

/// Paper-level samples of an annulus of the frame (polar grid through `h`) fitted with a
/// plane: the result of [`fit_paper_plane`].
#[derive(Debug, Clone)]
pub(crate) struct PaperPlane {
    /// Paper luma at the frame center.
    pub(crate) level: f64,
    /// Luma per image pixel along x and y.
    pub(crate) gradient: (f64, f64),
    /// Image position that `level` refers to.
    pub(crate) centre: (f64, f64),
    /// The samples that were taken to be paper: `(luma, x, y)`.
    pub(crate) paper: Vec<(f64, f64, f64)>,
}

impl PaperPlane {
    /// Multiplier that brings luma at an image position to the level at the frame center.
    pub(crate) fn gain(&self, x: f64, y: f64) -> f64 {
        let here = self.level
            + self.gradient.0 * (x - self.centre.0)
            + self.gradient.1 * (y - self.centre.1);
        (self.level / here.max(8.0)).clamp(0.5, 2.0)
    }

    /// Relative luma variation across a span of `extent` pixels.
    pub(crate) fn variation(&self, extent: f64) -> f64 {
        self.gradient.0.hypot(self.gradient.1) * extent / self.level.max(1.0)
    }
}

/// Sample an annulus around the frame center through `h`, take the pixels near the
/// `quantile` luma level to be paper and fit a plane to them. `None` if too few samples fall
/// inside the image.
pub(crate) fn fit_paper_plane(
    luma: PlaneRef<'_>,
    h: &Homography,
    radii: (f64, f64),
    quantile: f64,
) -> Option<PaperPlane> {
    let mut samples: Vec<(f64, f64, f64)> = Vec::with_capacity(1500);
    let rings = 6;
    for ri in 0..rings {
        let r = radii.0 + (radii.1 - radii.0) * (f64::from(ri) + 0.5) / f64::from(rings);
        let steps = 240;
        for k in 0..steps {
            let a =
                std::f64::consts::TAU * (f64::from(k) + 0.5 * f64::from(ri % 2)) / f64::from(steps);
            let Some((x, y)) = h.apply((r * a.cos(), r * a.sin())) else {
                continue;
            };
            if !luma.contains(x, y, 2.0) {
                continue;
            }
            samples.push((f64::from(luma.bilinear(x, y)), x, y));
        }
    }
    if samples.len() < 150 {
        return None;
    }
    let mut lumas: Vec<f64> = samples.iter().map(|s| s.0).collect();
    lumas.sort_by(f64::total_cmp);
    let idx = ((lumas.len() - 1) as f64 * quantile).round() as usize;
    let level = lumas[idx];
    let centre = h.apply((0.0, 0.0))?;
    // Paper pixels: not much darker (specks, dots) and not much brighter (glints).
    let mut paper: Vec<(f64, f64, f64)> = samples
        .iter()
        .copied()
        .filter(|s| s.0 >= 0.75 * level && s.0 <= 1.3 * level)
        .collect();
    let mut plane = fit_plane(&paper, centre).unwrap_or((level, 0.0, 0.0));
    for _ in 0..2 {
        let at = |x: f64, y: f64| plane.0 + plane.1 * (x - centre.0) + plane.2 * (y - centre.1);
        let kept: Vec<(f64, f64, f64)> = paper
            .iter()
            .copied()
            .filter(|s| (s.0 - at(s.1, s.2)).abs() <= 0.06 * level)
            .collect();
        if kept.len() < 100 {
            break;
        }
        paper = kept;
        plane = fit_plane(&paper, centre).unwrap_or(plane);
    }
    (plane.0.is_finite() && plane.0 >= 1.0).then_some(PaperPlane {
        level: plane.0,
        gradient: (plane.1, plane.2),
        centre,
        paper,
    })
}

/// Estimate the background from an annulus of the frame: [`fit_paper_plane`] plus the mean
/// chroma of the paper pixels. `None` if too few samples fall inside the image.
pub(crate) fn estimate_background(
    yuv: &YuvRef<'_>,
    h: &Homography,
    radii: (f64, f64),
    quantile: f64,
) -> Option<Background> {
    let plane = fit_paper_plane(yuv.luma(), h, radii, quantile)?;
    let (mut n, mut cb, mut cr) = (0.0, 0.0, 0.0);
    for &(_, x, y) in &plane.paper {
        let (b, r) = yuv.chroma(x, y);
        cb += f64::from(b);
        cr += f64::from(r);
        n += 1.0;
    }
    if n < 20.0 {
        return None;
    }
    Some(Background {
        luma: plane.level,
        cb: cb / n,
        cr: cr / n,
        gradient: plane.gradient,
        centre: plane.centre,
    })
}

/// Least-squares plane `a + gx (x - cx) + gy (y - cy)` through `(value, x, y)` samples.
fn fit_plane(samples: &[(f64, f64, f64)], centre: (f64, f64)) -> Option<(f64, f64, f64)> {
    let n = samples.len() as f64;
    if n < 8.0 {
        return None;
    }
    // Normal equations on centered coordinates.
    let (mut sx, mut sy, mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut sv, mut sxv, mut syv) = (0.0, 0.0, 0.0);
    for &(v, x, y) in samples {
        let (dx, dy) = (x - centre.0, y - centre.1);
        sx += dx;
        sy += dy;
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
        sv += v;
        sxv += dx * v;
        syv += dy * v;
    }
    let m = [[n, sx, sy, sv], [sx, sxx, sxy, sxv], [sy, sxy, syy, syv]];
    solve3(m).filter(|p| p.0.is_finite() && p.1.is_finite() && p.2.is_finite())
}

/// Gaussian elimination for a 3 × 3 system given as an augmented matrix.
fn solve3(mut m: [[f64; 4]; 3]) -> Option<(f64, f64, f64)> {
    for col in 0..3 {
        let pivot = (col..3).max_by(|&i, &j| m[i][col].abs().total_cmp(&m[j][col].abs()))?;
        if m[pivot][col].abs() < 1e-9 {
            return None;
        }
        m.swap(col, pivot);
        for row in col + 1..3 {
            let f = m[row][col] / m[col][col];
            let pivot_row = m[col];
            for (cell, &p) in m[row].iter_mut().zip(&pivot_row).skip(col) {
                *cell -= f * p;
            }
        }
    }
    let z = m[2][3] / m[2][2];
    let y = (m[1][3] - m[1][2] * z) / m[1][1];
    let x = (m[0][3] - m[0][1] * y - m[0][2] * z) / m[0][0];
    Some((x, y, z))
}

/// Result of [`ink_centroid`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct InkCentroid {
    pub(crate) at: (f64, f64),
    /// Total ink inside the window (luma units × pixels).
    pub(crate) mass: f64,
}

/// Mean-shift ink centroid: Gaussian-weighted (σ = `sigma`) ink inside a disc of `radius`
/// pixels, iterated from `start` until it settles (at most `iters` steps).
///
/// A dead band of 8 % of the background level keeps sensor noise out of the weights.
pub(crate) fn ink_centroid(
    luma: PlaneRef<'_>,
    start: (f64, f64),
    radius: f64,
    sigma: f64,
    bg: f64,
    iters: usize,
) -> Option<InkCentroid> {
    let dead = 0.08 * bg;
    let inv2s = 1.0 / (2.0 * sigma * sigma);
    let mut c = start;
    let mut mass = 0.0;
    for _ in 0..iters {
        if !luma.contains(c.0, c.1, radius + 3.0) {
            return None;
        }
        let half = radius.ceil() as i64 + 1;
        let (cx, cy) = (c.0.round() as i64, c.1.round() as i64);
        let (mut sw, mut sx, mut sy, mut sm) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for y in cy - half..=cy + half {
            let row = luma.row(y as usize);
            let dy = y as f64 - c.1;
            for x in cx - half..=cx + half {
                let dx = x as f64 - c.0;
                let d2 = dx * dx + dy * dy;
                if d2 > radius * radius {
                    continue;
                }
                let ink = (bg - f64::from(row[x as usize]) - dead).max(0.0);
                let w = ink * (-d2 * inv2s).exp();
                sw += w;
                sx += w * x as f64;
                sy += w * y as f64;
                sm += ink;
            }
        }
        if sw < 1e-6 {
            return None;
        }
        mass = sm;
        let next = (sx / sw, sy / sw);
        let moved = (next.0 - c.0).hypot(next.1 - c.1);
        c = next;
        if moved < 0.02 {
            break;
        }
    }
    Some(InkCentroid { at: c, mass })
}

/// Soft-disc ink mass `Σ (bg − luma)` over a disc of `radius` pixels centred at `c`
/// (one-pixel antialiased rim so the sum does not jump with sub-pixel motion).
pub(crate) fn ink_mass(luma: PlaneRef<'_>, c: (f64, f64), radius: f64, bg: f64) -> Option<f64> {
    if !luma.contains(c.0, c.1, radius + 3.0) {
        return None;
    }
    let half = radius.ceil() as i64 + 1;
    let (cx, cy) = (c.0.round() as i64, c.1.round() as i64);
    let mut sum = 0.0;
    for y in cy - half..=cy + half {
        let row = luma.row(y as usize);
        let dy = y as f64 - c.1;
        for x in cx - half..=cx + half {
            let dx = x as f64 - c.0;
            let d = (dx * dx + dy * dy).sqrt();
            let rim = (radius - d + 0.5).clamp(0.0, 1.0);
            if rim > 0.0 {
                sum += rim * (bg - f64::from(row[x as usize]));
            }
        }
    }
    Some(sum)
}

/// Mean (Cb, Cr) over a disc of `radius` pixels: the center plus two rings of samples.
pub(crate) fn mean_chroma(yuv: &YuvRef<'_>, c: (f64, f64), radius: f64) -> (f64, f64) {
    let mut sb = f64::from(yuv.chroma(c.0, c.1).0);
    let mut sr = f64::from(yuv.chroma(c.0, c.1).1);
    let mut n = 1.0;
    for (ring, count) in [(0.5, 6u32), (1.0, 12)] {
        for k in 0..count {
            let a = std::f64::consts::TAU * f64::from(k) / f64::from(count);
            let (b, r) = yuv.chroma(c.0 + radius * ring * a.cos(), c.1 + radius * ring * a.sin());
            sb += f64::from(b);
            sr += f64::from(r);
            n += 1.0;
        }
    }
    (sb / n, sr / n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Plane;

    fn dot_image(cx: f64, cy: f64, r: f64) -> Plane {
        let (w, h) = (40usize, 40usize);
        let mut data = vec![244u8; w * h];
        for y in 0..h {
            for x in 0..w {
                let a = (r - (x as f64 - cx).hypot(y as f64 - cy) + 0.5).clamp(0.0, 1.0);
                data[y * w + x] = (244.0 - a * 130.0) as u8;
            }
        }
        Plane::new(w, h, data)
    }

    #[test]
    fn centroid_finds_dot_and_mass_tracks_area() {
        let small = dot_image(20.3, 19.6, 2.5);
        let large = dot_image(20.3, 19.6, 3.4);
        let ls = small.view().expect("valid");
        let ll = large.view().expect("valid");
        let c = ink_centroid(ls, (21.0, 19.0), 4.0, 2.0, 244.0, 4).expect("centroid");
        assert!(
            (c.at.0 - 20.3).abs() < 0.1 && (c.at.1 - 19.6).abs() < 0.1,
            "{:?}",
            c.at
        );
        let ms = ink_mass(ls, c.at, 4.0, 244.0).expect("mass");
        let ml = ink_mass(ll, c.at, 4.0, 244.0).expect("mass");
        let ratio = ml / ms;
        assert!((ratio - (3.4f64 / 2.5).powi(2)).abs() < 0.12, "{ratio}");
    }

    #[test]
    fn empty_window_has_no_centroid() {
        let p = Plane::filled(40, 40, 244);
        assert!(ink_centroid(p.view().expect("valid"), (20.0, 20.0), 4.0, 2.0, 244.0, 3).is_none());
    }
}
