//! How wide are the shape's strokes? Morphological openings from an exact Euclidean
//! distance transform.
//!
//! A point has *local width* `>= 2ρ` when some disc of radius `ρ` lies inside the shape and
//! covers it. The set of such points is the morphological **opening** of the shape by a
//! disc of radius `ρ`: erode (keep the pixels whose distance to the nearest non-ink pixel is
//! at least `ρ`), then dilate by `ρ` (everything within `ρ` of what is left). Both steps
//! come from distance transforms, so an opening costs two linear passes over the
//! 1000 × 1000 raster and no medial axis has to be extracted.

use crate::path::DESIGN_SIZE;
use crate::raster::Raster;

const SIDE: usize = DESIGN_SIZE as usize;
/// Slack in pixels when asking whether an opening covers a lattice point, so that a point
/// on the shape's edge (whose own pixel may be background) is judged by the ink next to it.
const COVER_SLACK: f32 = 1.5;
/// Squared-distance value standing for "no seed".
const INF: f32 = 1e20;
/// Bisection steps of [`Thickness::width_quantile`].
const BISECT_STEPS: usize = 8;

/// Distance transform of a shape's raster, ready to open it at any radius.
#[derive(Debug, Clone)]
pub struct Thickness {
    /// Distance in pixels from each ink pixel to the nearest non-ink pixel (0 on
    /// background). The box border counts as background.
    radius: Vec<f32>,
    max_radius: f32,
    scratch: Scratch,
}

impl Thickness {
    /// Compute the distance transform of `raster`.
    pub fn new(raster: &Raster) -> Thickness {
        let w = SIDE + 2;
        // Padded by one pixel of background so the box edge is an edge of the shape.
        let mut g = vec![0f32; w * w];
        for j in 0..SIDE {
            for i in 0..SIDE {
                if raster.at(i as isize, j as isize) {
                    g[(j + 1) * w + i + 1] = INF;
                }
            }
        }
        let mut scratch = Scratch::new(w);
        squared_edt(&mut g, w, &mut scratch);
        let mut radius = Vec::with_capacity(SIDE * SIDE);
        for j in 0..SIDE {
            for i in 0..SIDE {
                radius.push(g[(j + 1) * w + i + 1].sqrt());
            }
        }
        let max_radius = radius.iter().copied().fold(0.0, f32::max);
        Thickness {
            radius,
            max_radius,
            scratch,
        }
    }

    /// Radius in design units of the largest disc that fits in the shape.
    pub fn max_radius(&self) -> f64 {
        f64::from(self.max_radius)
    }

    /// How many of the design-box `points` are *not* covered by a disc of radius `rho`
    /// (design units) that lies inside the shape, i.e. have local width `< 2 rho`.
    pub fn uncovered(&mut self, rho: f64, points: &[(f64, f64)]) -> usize {
        let rho = rho as f32;
        let w = SIDE;
        // Dilation: distance to the nearest pixel that survives the erosion.
        let mut g = vec![INF; w * w];
        let mut seeds = false;
        for (cell, &r) in g.iter_mut().zip(&self.radius) {
            if r >= rho && r > 0.0 {
                *cell = 0.0;
                seeds = true;
            }
        }
        if !seeds {
            return points.len();
        }
        squared_edt(&mut g, w, &mut self.scratch);
        let reach = (rho + COVER_SLACK) * (rho + COVER_SLACK);
        let side = SIDE as isize;
        points
            .iter()
            .filter(|&&(x, y)| {
                let (ci, cj) = (x.floor() as isize, y.floor() as isize);
                let mut best = INF;
                for dj in -1..=1 {
                    for di in -1..=1 {
                        let (i, j) = (ci + di, cj + dj);
                        if (0..side).contains(&i) && (0..side).contains(&j) {
                            best = best.min(g[(j * side + i) as usize]);
                        }
                    }
                }
                best > reach
            })
            .count()
    }

    /// The largest local width (design units, `2 rho`) such that at most `allowed` of
    /// `points` are thinner; `0` if even the smallest disc leaves more than that
    /// uncovered. `floor` is tried first: if `floor` units already leave more than
    /// `allowed` points uncovered the answer lies below it, otherwise at or above.
    ///
    /// The result is exact to about `max_radius / 256` design units.
    pub fn width_quantile(&mut self, points: &[(f64, f64)], allowed: usize, floor: f64) -> f64 {
        let (mut lo, mut hi);
        if self.uncovered(floor / 2.0, points) <= allowed {
            lo = floor / 2.0;
            hi = f64::from(self.max_radius) + 1.0;
            if lo >= hi {
                return 2.0 * lo;
            }
        } else {
            lo = 0.0;
            hi = floor / 2.0;
        }
        for _ in 0..BISECT_STEPS {
            let mid = 0.5 * (lo + hi);
            if self.uncovered(mid, points) <= allowed {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        2.0 * lo
    }
}

/// In-place squared Euclidean distance transform of a `w x w` grid whose cells hold `0`
/// (seed) or [`INF`]; afterwards each cell holds the squared distance to the nearest seed
/// (Felzenszwalb and Huttenlocher, linear time).
fn squared_edt(g: &mut [f32], w: usize, scratch: &mut Scratch) {
    let mut line = vec![0f32; w];
    let mut out = vec![0f32; w];
    for x in 0..w {
        for y in 0..w {
            line[y] = g[y * w + x];
        }
        scratch.transform(&line, &mut out);
        for y in 0..w {
            g[y * w + x] = out[y];
        }
    }
    for y in 0..w {
        line.copy_from_slice(&g[y * w..(y + 1) * w]);
        scratch.transform(&line, &mut out);
        g[y * w..(y + 1) * w].copy_from_slice(&out);
    }
}

/// Reusable buffers of the 1-D lower-envelope transform.
#[derive(Debug, Clone)]
struct Scratch {
    v: Vec<usize>,
    z: Vec<f64>,
}

impl Scratch {
    fn new(n: usize) -> Scratch {
        Scratch {
            v: vec![0; n],
            z: vec![0.0; n + 1],
        }
    }

    /// `out[q] = min_p (f[p] + (q - p)^2)`.
    fn transform(&mut self, f: &[f32], out: &mut [f32]) {
        let n = f.len();
        let (v, z) = (&mut self.v, &mut self.z);
        // A line without seeds stays without seeds.
        if f.iter().all(|&x| x >= INF) {
            out.copy_from_slice(f);
            return;
        }
        let mut k = 0;
        v[0] = 0;
        z[0] = f64::NEG_INFINITY;
        z[1] = f64::INFINITY;
        for q in 1..n {
            let qf = q as f64;
            let fq = f64::from(f[q]);
            let mut s;
            loop {
                let p = v[k];
                let pf = p as f64;
                s = ((fq + qf * qf) - (f64::from(f[p]) + pf * pf)) / (2.0 * qf - 2.0 * pf);
                if s <= z[k] {
                    k -= 1;
                } else {
                    break;
                }
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = f64::INFINITY;
        }
        k = 0;
        for (q, o) in out.iter_mut().enumerate().take(n) {
            while z[k + 1] < q as f64 {
                k += 1;
            }
            let d = q as f32 - v[k] as f32;
            *o = (d * d + f[v[k]]).min(INF);
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // reason: tests compare exact, hand-written coordinates
mod tests {
    use super::*;
    use crate::path::{ViewBox, parse_path};

    fn bar(width: f32) -> Raster {
        let line = parse_path("M 100 500 L 900 500", &ViewBox::DESIGN).unwrap();
        Raster::from_path(&line, Some(width)).unwrap()
    }

    #[test]
    fn distance_transform_of_a_square() {
        let sq = parse_path(
            "M 200 200 L 800 200 L 800 800 L 200 800 Z",
            &ViewBox::DESIGN,
        )
        .unwrap();
        let t = Thickness::new(&Raster::from_path(&sq, None).unwrap());
        assert!((t.max_radius() - 300.0).abs() < 1.5, "{}", t.max_radius());
    }

    #[test]
    fn opening_separates_wide_from_narrow() {
        let mut t = Thickness::new(&bar(100.0));
        // Points along the bar, centre line and 45 units off it.
        let pts: Vec<(f64, f64)> = (0..10)
            .flat_map(|i| {
                let x = 300.0 + 40.0 * f64::from(i);
                [(x, 500.0), (x, 545.0), (x, 455.0)]
            })
            .collect();
        assert_eq!(t.uncovered(45.0, &pts), 0);
        assert_eq!(t.uncovered(60.0, &pts), pts.len());
        // A point outside the bar is never covered.
        assert_eq!(t.uncovered(10.0, &[(500.0, 100.0)]), 1);
    }

    #[test]
    fn width_quantile_recovers_the_bar_width() {
        let mut t = Thickness::new(&bar(100.0));
        let pts: Vec<(f64, f64)> = (0..20)
            .map(|i| (300.0 + 20.0 * f64::from(i), 500.0))
            .collect();
        let w = t.width_quantile(&pts, 0, 40.0);
        assert!((w - 100.0).abs() < 6.0, "{w}");
        let narrow = {
            let mut t = Thickness::new(&bar(30.0));
            t.width_quantile(&pts, 0, 40.0)
        };
        assert!((narrow - 30.0).abs() < 6.0, "{narrow}");
    }

    #[test]
    fn empty_shape_has_no_width() {
        let tri = parse_path("M 10 10 L 10.2 10 L 10 10.2 Z", &ViewBox::DESIGN).unwrap();
        let mut t = Thickness::new(&Raster::from_path(&tri, None).unwrap());
        assert_eq!(t.uncovered(5.0, &[(500.0, 500.0)]), 1);
        assert_eq!(t.width_quantile(&[(500.0, 500.0)], 0, 40.0), 0.0);
    }
}
