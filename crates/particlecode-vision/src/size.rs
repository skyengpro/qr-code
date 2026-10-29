//! Small-or-large decision for dots.
//!
//! A dot's size shows up as how much ink it carries relative to the calibration swatch of
//! the same hue (the swatch radius, 0.42, lies between the small 0.36 and large 0.48 dot).
//! How the two sizes separate depends on the blur: on a sharp image a wide measurement
//! window catches the whole dot and tells them apart, while under blur the wide window is
//! swamped by the tails of the six neighbours and a window the size of a small dot is the
//! one that still separates. Instead of guessing the blur, every dot is measured through two
//! windows and a Fisher discriminant fitted to the frame's own two clusters picks the best
//! combination: [`SizeModel`].

use particlecode_core::layout::LatticePoint;

/// Score difference to the neighbours' median beyond which the neighbour rule decides.
pub(crate) const NEIGHBOUR_RULE: f64 = 0.6;
/// Confidence floor when the neighbour rule agrees with the boundary.
pub(crate) const AGREEMENT_CONFIDENCE: f64 = 0.6;
/// Confidence ceiling when the neighbour rule overrules the boundary.
pub(crate) const CONFLICT_CONFIDENCE: f64 = 0.3;

/// Natural logs of a dot's ink mass over its hue's swatch mass, for the core and the wide
/// window.
pub(crate) type Feature = [f64; 2];

/// Frame-wide decision boundary between small and large dots in feature space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SizeModel {
    /// Midpoint between the two cluster centers.
    mid: Feature,
    /// Discriminant direction, scaled so that the score is -1 at the small center and +1 at
    /// the large one.
    axis: Feature,
    /// Within-cluster standard deviation of the score.
    sigma: f64,
    /// Whether the model was fitted to two clusters in the data (otherwise nominal).
    pub(crate) fitted: bool,
    /// Distance between the two cluster centers in the features' natural-log units.
    pub(crate) separation: f64,
}

impl SizeModel {
    /// The model assumed when the frame offers no two clusters: a sharp image, threshold at
    /// the swatch's own ink mass in the wide window.
    pub(crate) fn nominal() -> SizeModel {
        SizeModel {
            mid: [0.0, 0.0],
            axis: [0.0, 1.0 / 0.25],
            sigma: 0.3,
            fitted: false,
            separation: 0.5,
        }
    }

    /// Fit two clusters to the features of a frame's dots. Falls back to
    /// [`SizeModel::nominal`] when they do not look like small and large dots.
    pub(crate) fn fit(features: &[Feature]) -> SizeModel {
        let n = features.len();
        if n < 12 {
            return SizeModel::nominal();
        }
        let nf = n as f64;
        let mean = [
            features.iter().map(|f| f[0]).sum::<f64>() / nf,
            features.iter().map(|f| f[1]).sum::<f64>() / nf,
        ];
        let (mut c00, mut c01, mut c11) = (0.0, 0.0, 0.0);
        for f in features {
            let d = [f[0] - mean[0], f[1] - mean[1]];
            c00 += d[0] * d[0];
            c01 += d[0] * d[1];
            c11 += d[1] * d[1];
        }
        // Principal direction of the whole cloud; larger projection = larger dot.
        let theta = 0.5 * (2.0 * c01).atan2(c00 - c11);
        let mut v = [theta.cos(), theta.sin()];
        if v[0] + v[1] < 0.0 {
            v = [-v[0], -v[1]];
        }
        // One-dimensional two-means along it to initialise the clusters.
        let proj: Vec<f64> = features
            .iter()
            .map(|f| (f[0] - mean[0]) * v[0] + (f[1] - mean[1]) * v[1])
            .collect();
        let mut thr = 0.0;
        let (mut lo, mut hi) = (0.0, 0.0);
        for _ in 0..12 {
            let (mut sl, mut nl, mut sh, mut nh) = (0.0, 0.0, 0.0, 0.0);
            for &t in &proj {
                if t < thr {
                    sl += t;
                    nl += 1.0;
                } else {
                    sh += t;
                    nh += 1.0;
                }
            }
            if nl < 1.0 || nh < 1.0 {
                return SizeModel::nominal();
            }
            lo = sl / nl;
            hi = sh / nh;
            thr = 0.5 * (lo + hi);
        }
        let mut centers = [
            [mean[0] + lo * v[0], mean[1] + lo * v[1]],
            [mean[0] + hi * v[0], mean[1] + hi * v[1]],
        ];
        // Lloyd iterations in the plane.
        let mut member = vec![false; n];
        for _ in 0..10 {
            let (mut sums, mut counts) = ([[0.0f64; 2]; 2], [0.0f64; 2]);
            for (i, f) in features.iter().enumerate() {
                let d0 = (f[0] - centers[0][0]).powi(2) + (f[1] - centers[0][1]).powi(2);
                let d1 = (f[0] - centers[1][0]).powi(2) + (f[1] - centers[1][1]).powi(2);
                let big = d1 < d0;
                member[i] = big;
                let k = usize::from(big);
                sums[k][0] += f[0];
                sums[k][1] += f[1];
                counts[k] += 1.0;
            }
            if counts[0] < 1.0 || counts[1] < 1.0 {
                return SizeModel::nominal();
            }
            centers = [
                [sums[0][0] / counts[0], sums[0][1] / counts[0]],
                [sums[1][0] / counts[1], sums[1][1] / counts[1]],
            ];
        }
        let n_big = member.iter().filter(|&&b| b).count() as f64;
        let frac_big = n_big / nf;
        let diff = [centers[1][0] - centers[0][0], centers[1][1] - centers[0][1]];
        let separation = diff[0].hypot(diff[1]);
        if !(0.12..=1.5).contains(&separation)
            || !(0.15..=0.85).contains(&frac_big)
            || diff[0] + diff[1] <= 0.0
        {
            return SizeModel::nominal();
        }
        // Fisher discriminant: w = S_w^-1 (c_big - c_small), S_w the pooled within-cluster
        // covariance (ridge-regularised).
        let (mut s00, mut s01, mut s11) = (0.0, 0.0, 0.0);
        for (f, &big) in features.iter().zip(&member) {
            let c = centers[usize::from(big)];
            let d = [f[0] - c[0], f[1] - c[1]];
            s00 += d[0] * d[0];
            s01 += d[0] * d[1];
            s11 += d[1] * d[1];
        }
        let ridge = 1e-4 + 0.02 * separation * separation;
        let (s00, s01, s11) = (s00 / nf + ridge, s01 / nf, s11 / nf + ridge);
        let det = s00 * s11 - s01 * s01;
        if det <= 1e-12 {
            return SizeModel::nominal();
        }
        let w = [
            (s11 * diff[0] - s01 * diff[1]) / det,
            (-s01 * diff[0] + s00 * diff[1]) / det,
        ];
        let mid = [
            0.5 * (centers[0][0] + centers[1][0]),
            0.5 * (centers[0][1] + centers[1][1]),
        ];
        let half = 0.5 * (diff[0] * w[0] + diff[1] * w[1]);
        if half <= 1e-9 {
            return SizeModel::nominal();
        }
        let axis = [w[0] / half, w[1] / half];
        // Within-cluster scatter of the score.
        let mut sq = 0.0;
        for (f, &big) in features.iter().zip(&member) {
            let target = if big { 1.0 } else { -1.0 };
            let s = (f[0] - mid[0]) * axis[0] + (f[1] - mid[1]) * axis[1];
            sq += (s - target).powi(2);
        }
        let sigma = (sq / nf).sqrt().max(0.08);
        SizeModel {
            mid,
            axis,
            sigma,
            fitted: true,
            separation,
        }
    }

    /// Score of a feature: about -1 for a small dot, +1 for a large one.
    pub(crate) fn score(&self, f: &Feature) -> f64 {
        (f[0] - self.mid[0]) * self.axis[0] + (f[1] - self.mid[1]) * self.axis[1]
    }

    /// Size bit and confidence `0..=1` for a score, with the local boundary at `offset`.
    /// The confidence reaches 1 at 2.5 within-cluster standard deviations from the boundary.
    pub(crate) fn decide(&self, score: f64, offset: f64) -> (u8, f64) {
        let s = score - offset;
        (
            u8::from(s > 0.0),
            (s.abs() / (2.5 * self.sigma)).clamp(0.0, 1.0),
        )
    }

    /// [`SizeModel::decide`] refined by the spec's neighbour rule: a dot well above the
    /// median score of its (up to) six lattice neighbours is large, one well below is
    /// small, whatever the boundary says. Blur, glare and the breathing wave move a dot
    /// *and* its neighbours together, so the difference to them survives what the absolute
    /// level does not.
    ///
    /// The spec states the rule as mass ratios (large above 1.35×, small below 0.75× the
    /// neighbours' median); scores are linear in ln(mass) with the two clusters at ∓1, and
    /// the same cut sits at [`NEIGHBOUR_RULE`] score units. When the rule agrees with the
    /// boundary the confidence is raised to at least [`AGREEMENT_CONFIDENCE`]; when it
    /// overrules it, lowered to at most [`CONFLICT_CONFIDENCE`], so contested dots become
    /// erasures for Reed-Solomon instead of silent errors.
    pub(crate) fn decide_with_neighbours(
        &self,
        score: f64,
        offset: f64,
        neighbour_median: Option<f64>,
    ) -> (u8, f64) {
        let (bit, conf) = self.decide(score, offset);
        let Some(median) = neighbour_median else {
            return (bit, conf);
        };
        let rel = score - median;
        let rule = if rel > NEIGHBOUR_RULE {
            Some(1)
        } else if rel < -NEIGHBOUR_RULE {
            Some(0)
        } else {
            None
        };
        match rule {
            Some(r) if r == bit => (bit, conf.max(AGREEMENT_CONFIDENCE)),
            Some(r) => (r, conf.min(CONFLICT_CONFIDENCE)),
            None => (bit, conf),
        }
    }
}

/// Per-dot boundary offsets (in score units) that follow slow variations of the measured
/// scale across the field.
///
/// Focus falls off with tilt, the breathing wave swells and shrinks dots together, exposure
/// varies. The field is cut into cells; where a cell holds enough dots on both sides of the
/// frame-wide boundary, the midpoint of its small and large means is the cell's boundary.
/// Each dot gets a Gaussian-weighted blend of the cells around it (twice, so the split
/// improves), and zero where there is not enough evidence.
pub(crate) fn local_offsets(pts: &[LatticePoint], scores: &[f64]) -> Vec<f64> {
    const CELL: f64 = 5.0;
    const SPREAD: f64 = 6.0;
    let extent = pts
        .iter()
        .fold(0.0f64, |a, p| a.max(p.at.x.abs()).max(p.at.y.abs()))
        + 1.0;
    let n = ((2.0 * extent / CELL).ceil() as usize).max(1);
    let cell_of = |p: &LatticePoint| -> (usize, usize) {
        let cx = (((p.at.x + extent) / CELL) as usize).min(n - 1);
        let cy = (((p.at.y + extent) / CELL) as usize).min(n - 1);
        (cx, cy)
    };
    let mut off = vec![0.0; pts.len()];
    for _ in 0..2 {
        // (sum small, count small, sum large, count large) per cell.
        let mut cells = vec![(0.0f64, 0.0f64, 0.0f64, 0.0f64); n * n];
        for (i, p) in pts.iter().enumerate() {
            let s = scores[i];
            if !s.is_finite() {
                continue;
            }
            let (cx, cy) = cell_of(p);
            let c = &mut cells[cy * n + cx];
            if s > off[i] {
                c.2 += s;
                c.3 += 1.0;
            } else {
                c.0 += s;
                c.1 += 1.0;
            }
        }
        // Boundary and weight per cell.
        let cell_off: Vec<Option<(f64, f64)>> = cells
            .iter()
            .map(|c| {
                (c.1 >= 4.0 && c.3 >= 4.0).then(|| (0.5 * (c.0 / c.1 + c.2 / c.3), c.1.min(c.3)))
            })
            .collect();
        let reach = (2.5 * SPREAD / CELL).ceil() as i64;
        off = pts
            .iter()
            .map(|p| {
                let (cx, cy) = cell_of(p);
                let (mut sw, mut so) = (0.0, 0.0);
                for dy in -reach..=reach {
                    for dx in -reach..=reach {
                        let (x, y) = (cx as i64 + dx, cy as i64 + dy);
                        if x < 0 || y < 0 || x >= n as i64 || y >= n as i64 {
                            continue;
                        }
                        if let Some((o, w)) = cell_off[y as usize * n + x as usize] {
                            let centre = (
                                (x as f64 + 0.5) * CELL - extent,
                                (y as f64 + 0.5) * CELL - extent,
                            );
                            let d2 = (centre.0 - p.at.x).powi(2) + (centre.1 - p.at.y).powi(2);
                            let k = w * (-d2 / (2.0 * SPREAD * SPREAD)).exp();
                            sw += k;
                            so += k * o;
                        }
                    }
                }
                if sw > 3.0 {
                    (so / sw).clamp(-0.6, 0.6)
                } else {
                    0.0
                }
            })
            .collect();
    }
    off
}

#[cfg(test)]
mod tests {
    use super::*;
    use particlecode_core::layout::axial_to_point;

    /// Deterministic pseudo-noise in [-1, 1].
    fn noise(i: usize, salt: usize) -> f64 {
        let x = (i * 2_654_435_761 + salt * 40_503) % 10_007;
        x as f64 / 5003.5 - 1.0
    }

    fn two_clusters(small: Feature, large: Feature, spread: f64, n: usize) -> Vec<Feature> {
        (0..n)
            .map(|i| {
                let c = if i % 2 == 0 { small } else { large };
                [c[0] + spread * noise(i, 1), c[1] + spread * noise(i, 2)]
            })
            .collect()
    }

    #[test]
    fn fits_a_sharp_frame_where_only_the_wide_window_separates() {
        // Core window: both sizes fill it. Wide window: 0.735 vs 1.3 of the swatch.
        let f = two_clusters([-0.02, (0.735f64).ln()], [0.0, (1.3f64).ln()], 0.03, 200);
        let m = SizeModel::fit(&f);
        assert!(m.fitted);
        assert!(m.score(&[-0.02, (0.735f64).ln()]) < -0.8);
        assert!(m.score(&[0.0, (1.3f64).ln()]) > 0.8);
    }

    #[test]
    fn fits_a_blurred_frame_where_the_core_window_carries_the_signal() {
        // Wide window polluted by neighbours (high variance); core window clean.
        let mut f = Vec::new();
        for i in 0..300 {
            let big = i % 2 == 0;
            let core = if big { 0.10 } else { -0.12 } + 0.02 * noise(i, 3);
            let wide = if big { 0.30 } else { 0.18 } + 0.12 * noise(i, 4);
            f.push([core, wide]);
        }
        let m = SizeModel::fit(&f);
        assert!(m.fitted);
        let right = f
            .iter()
            .enumerate()
            .filter(|(i, x)| (m.score(x) > 0.0) == (i % 2 == 0))
            .count();
        assert!(right >= 295, "{right} of 300");
    }

    #[test]
    fn single_cluster_falls_back_to_nominal() {
        let f = two_clusters([0.0, 0.0], [0.0, 0.0], 0.02, 100);
        assert!(!SizeModel::fit(&f).fitted);
        assert!(!SizeModel::fit(&f[..5]).fitted);
    }

    #[test]
    fn decide_reports_confidence() {
        let m = SizeModel::fit(&two_clusters([-0.1, -0.3], [0.1, 0.3], 0.02, 100));
        let (bit, conf) = m.decide(1.0, 0.0);
        assert_eq!(bit, 1);
        assert!(conf > 0.9);
        let (bit, conf) = m.decide(-0.02, 0.0);
        assert_eq!(bit, 0);
        assert!(conf < 0.3);
        // A shifted local boundary flips the decision.
        assert_eq!(m.decide(0.3, 0.5).0, 0);
    }

    #[test]
    fn neighbour_rule_overrules_a_shifted_boundary() {
        let m = SizeModel::fit(&two_clusters([-0.1, -0.3], [0.1, 0.3], 0.02, 100));
        // Everything in this region reads high (glare thins the ink the other way, say): a
        // dot at +0.3 among neighbours at +1 is a small one even though the boundary says large.
        let (bit, conf) = m.decide_with_neighbours(0.3, 0.0, Some(1.2));
        assert_eq!(bit, 0);
        assert!(conf <= CONFLICT_CONFIDENCE);
        // Agreement raises the confidence of a dot near the boundary.
        let (bit, conf) = m.decide_with_neighbours(0.2, 0.0, Some(-1.0));
        assert_eq!(bit, 1);
        assert!(conf >= AGREEMENT_CONFIDENCE);
        // Neighbours at the same level: the boundary decides.
        assert_eq!(
            m.decide_with_neighbours(0.2, 0.0, Some(0.0)),
            m.decide(0.2, 0.0)
        );
        assert_eq!(m.decide_with_neighbours(0.2, 0.0, None), m.decide(0.2, 0.0));
    }

    #[test]
    fn local_offsets_follow_a_gain_gradient() {
        // A hex patch where the score of both classes drifts upwards with x.
        let mut pts = Vec::new();
        for r in -12..=12 {
            for q in -14..=14 {
                pts.push(LatticePoint {
                    q,
                    r,
                    at: axial_to_point(q, r),
                });
            }
        }
        let scores: Vec<f64> = pts
            .iter()
            .enumerate()
            .map(|(i, p)| (if noise(i, 5) > 0.0 { 1.0 } else { -1.0 }) + 0.02 * p.at.x)
            .collect();
        let off = local_offsets(&pts, &scores);
        let (left, right) = (
            pts.iter().position(|p| p.q == -10 && p.r == 0).unwrap_or(0),
            pts.iter().position(|p| p.q == 10 && p.r == 0).unwrap_or(0),
        );
        assert!(
            off[right] - off[left] > 0.15,
            "{} {}",
            off[left],
            off[right]
        );
    }
}
