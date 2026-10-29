//! Per-frame colour calibration from the 46 hue swatches.

use crate::error::VisionError;
use crate::util::median;

/// What the calibration ring said about this frame's colours and exposure.
#[derive(Debug, Clone, PartialEq)]
pub struct Calibration {
    /// Luma of the empty background (paper).
    pub background_luma: f64,
    /// Luma at the white swatch, if it was visible.
    pub white_luma: Option<f64>,
    /// Luma at the black swatch, if it was visible.
    pub black_luma: Option<f64>,
    /// Chroma (Cb, Cr) of the background, subtracted from every measurement as the
    /// white-balance offset.
    pub chroma_offset: (f64, f64),
    /// Per-hue centroid in offset-corrected (Cb, Cr).
    pub hue_centroids: [(f64, f64); 4],
    /// Per-hue mean luma at the swatch centers.
    pub hue_luma: [f64; 4],
    /// Pooled RMS scatter of the swatches around their hue centroid (chroma units).
    pub spread: f64,
    /// Smallest distance between two hue centroids divided by `spread`; the frame is
    /// rejected below 2.
    pub separation: f64,
    /// Median swatch ink mass per hue, in luma units × pitch², for the two measurement
    /// windows (index 0: the core window, index 1: the wide window).
    pub swatch_mass: [[f64; 4]; 2],
}

/// One measured hue swatch.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SwatchMeas {
    pub(crate) hue: usize,
    pub(crate) chroma: (f64, f64),
    pub(crate) mass: [f64; 2],
    pub(crate) luma: f64,
}

/// Fixed part of the calibration needed to classify dots.
#[derive(Debug, Clone)]
pub(crate) struct HueModel {
    angle: [f64; 4],
    magnitude: [f64; 4],
}

impl HueModel {
    /// Whether the four hue labels go around the chroma wheel in the order the palette has
    /// them: with increasing `CbCr` angle, blue, magenta, orange, green — labels 0, 3, 2, 1.
    ///
    /// Color casts and white balance rotate and stretch the wheel but never reverse it; a
    /// mirrored view does (every label `s % 4` along a swatch ring becomes `-s % 4`), and that
    /// is the one thing the mirror-symmetric halo geometry cannot reveal.
    pub(crate) fn wheel_order_ok(&self) -> bool {
        let mut by_angle: [usize; 4] = [0, 1, 2, 3];
        by_angle.sort_by(|&a, &b| self.angle[a].total_cmp(&self.angle[b]));
        let at = by_angle.iter().position(|&l| l == 0).unwrap_or(0);
        (0..4).all(|i| by_angle[(at + i) % 4] == [0, 3, 2, 1][i])
    }

    /// Hue index and confidence `0..=1` for an offset-corrected chroma vector.
    ///
    /// The decision uses the *direction* in the `CbCr` plane: blur and dot size shrink a
    /// dot's chroma towards zero without rotating it, so angles are invariant where
    /// distances are not.
    pub(crate) fn classify(&self, chroma: (f64, f64)) -> (u8, f64) {
        let mag = chroma.0.hypot(chroma.1);
        let a = chroma.1.atan2(chroma.0);
        let mut best = (0usize, f64::MAX);
        let mut second = f64::MAX;
        for (k, &ak) in self.angle.iter().enumerate() {
            let mut d = (a - ak).abs();
            if d > std::f64::consts::PI {
                d = std::f64::consts::TAU - d;
            }
            if d < best.1 {
                second = best.1;
                best = (k, d);
            } else if d < second {
                second = d;
            }
        }
        let margin = if second + best.1 > 1e-9 {
            (second - best.1) / (second + best.1)
        } else {
            0.0
        };
        let hue_conf = (margin / 0.6).clamp(0.0, 1.0);
        // A chroma much weaker than the swatch's is washed out (glare, shadow): full
        // confidence above 60 % of the swatch's, none below 25 %. Blur alone leaves
        // small dots at about 70-90 %.
        let rel = mag / self.magnitude[best.0].max(1e-6);
        let strength = ((rel - 0.25) / 0.35).clamp(0.0, 1.0);
        (best.0 as u8, hue_conf * strength)
    }
}

impl Calibration {
    /// Build the calibration from swatch measurements. `Err(Lighting)` when a hue has too
    /// few swatches, a hue has almost no chroma, or two hues overlap.
    pub(crate) fn from_swatches(
        meas: &[SwatchMeas],
        background_luma: f64,
        chroma_offset: (f64, f64),
        white_luma: Option<f64>,
        black_luma: Option<f64>,
    ) -> Result<(Calibration, HueModel), VisionError> {
        let mut centroids = [(0.0, 0.0); 4];
        let mut hue_luma = [0.0; 4];
        let mut swatch_mass = [[0.0; 4]; 2];
        let mut sq_sum = 0.0f64;
        let mut sq_n = 0.0f64;
        for hue in 0..4 {
            let mine: Vec<&SwatchMeas> = meas.iter().filter(|m| m.hue == hue).collect();
            if mine.len() < 5 {
                return Err(VisionError::Lighting);
            }
            let med = (
                median(&mut mine.iter().map(|m| m.chroma.0).collect::<Vec<_>>()),
                median(&mut mine.iter().map(|m| m.chroma.1).collect::<Vec<_>>()),
            );
            let med_mag = med.0.hypot(med.1);
            // Drop swatches that disagree with the median (a dot of another colour, a
            // glare spot, a mislocated centroid).
            let keep: Vec<&SwatchMeas> = mine
                .iter()
                .copied()
                .filter(|m| {
                    (m.chroma.0 - med.0).hypot(m.chroma.1 - med.1) <= (0.35 * med_mag).max(12.0)
                })
                .collect();
            if keep.len() < 4 {
                return Err(VisionError::Lighting);
            }
            let n = keep.len() as f64;
            let c = (
                keep.iter().map(|m| m.chroma.0).sum::<f64>() / n,
                keep.iter().map(|m| m.chroma.1).sum::<f64>() / n,
            );
            for m in &keep {
                sq_sum += (m.chroma.0 - c.0).powi(2) + (m.chroma.1 - c.1).powi(2);
                sq_n += 1.0;
            }
            centroids[hue] = c;
            hue_luma[hue] = keep.iter().map(|m| m.luma).sum::<f64>() / n;
            for (w, slot) in swatch_mass.iter_mut().enumerate() {
                slot[hue] = median(&mut keep.iter().map(|m| m.mass[w]).collect::<Vec<_>>());
            }
        }
        let spread = (sq_sum / sq_n.max(1.0)).sqrt();
        let mut min_dist = f64::MAX;
        for i in 0..4 {
            for j in i + 1..4 {
                let d = (centroids[i].0 - centroids[j].0).hypot(centroids[i].1 - centroids[j].1);
                min_dist = min_dist.min(d);
            }
        }
        let weakest = centroids
            .iter()
            .map(|c| c.0.hypot(c.1))
            .fold(f64::MAX, f64::min);
        // The floor on the spread keeps a perfectly clean synthetic frame from dividing by 0.
        let separation = min_dist / spread.max(1.0);
        if weakest < 10.0
            || separation < 2.0
            || swatch_mass
                .iter()
                .flatten()
                .any(|&m| m <= 0.0 || m.is_nan())
        {
            return Err(VisionError::Lighting);
        }
        let model = HueModel {
            angle: centroids.map(|c| c.1.atan2(c.0)),
            magnitude: centroids.map(|c| c.0.hypot(c.1)),
        };
        Ok((
            Calibration {
                background_luma,
                white_luma,
                black_luma,
                chroma_offset,
                hue_centroids: centroids,
                hue_luma,
                spread,
                separation,
                swatch_mass,
            },
            model,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meas(centers: [(f64, f64); 4], wobble: f64) -> Vec<SwatchMeas> {
        let mut v = Vec::new();
        for (hue, c) in centers.iter().enumerate() {
            for i in 0..12 {
                let w = if i % 2 == 0 { wobble } else { -wobble };
                v.push(SwatchMeas {
                    hue,
                    chroma: (c.0 + w, c.1 - w),
                    mass: [100.0; 2],
                    luma: 100.0,
                });
            }
        }
        v
    }

    const GOOD: [(f64, f64); 4] = [(83.0, -56.0), (-46.0, -51.0), (-73.0, 76.0), (38.0, 77.0)];

    #[test]
    fn good_lighting_calibrates_and_classifies() {
        let (cal, model) =
            Calibration::from_swatches(&meas(GOOD, 2.0), 244.0, (0.0, 0.0), None, None)
                .expect("ok");
        assert!(cal.separation > 10.0);
        for (k, c) in GOOD.iter().enumerate() {
            // Attenuated by blur: same direction, 70 % of the length.
            let (h, conf) = model.classify((c.0 * 0.7, c.1 * 0.7));
            assert_eq!(usize::from(h), k);
            assert!(conf > 0.8, "{conf}");
        }
    }

    #[test]
    fn wheel_order_tells_mirrored_labels_apart() {
        let (_, model) =
            Calibration::from_swatches(&meas(GOOD, 1.0), 244.0, (0.0, 0.0), None, None)
                .expect("ok");
        assert!(model.wheel_order_ok());
        // Labels 1 and 3 swapped, as a mirrored swatch ring would give.
        let swapped = [GOOD[0], GOOD[3], GOOD[2], GOOD[1]];
        let (_, model) =
            Calibration::from_swatches(&meas(swapped, 1.0), 244.0, (0.0, 0.0), None, None)
                .expect("ok");
        assert!(!model.wheel_order_ok());
        // A color cast that rotates the wheel keeps the order.
        let rot = |c: (f64, f64)| (c.0 * 0.8 - c.1 * 0.6, c.0 * 0.6 + c.1 * 0.8);
        let rotated = GOOD.map(rot);
        let (_, model) =
            Calibration::from_swatches(&meas(rotated, 1.0), 244.0, (0.0, 0.0), None, None)
                .expect("ok");
        assert!(model.wheel_order_ok());
    }

    #[test]
    fn overlapping_hues_are_rejected() {
        let washed = GOOD.map(|c| (c.0 * 0.04 + 3.0, c.1 * 0.04));
        assert_eq!(
            Calibration::from_swatches(&meas(washed, 1.0), 244.0, (0.0, 0.0), None, None).err(),
            Some(VisionError::Lighting)
        );
        let noisy = meas(GOOD, 60.0);
        assert!(Calibration::from_swatches(&noisy, 244.0, (0.0, 0.0), None, None).is_err());
    }

    #[test]
    fn too_few_swatches_are_rejected() {
        let few: Vec<SwatchMeas> = meas(GOOD, 1.0).into_iter().filter(|m| m.hue != 2).collect();
        assert!(Calibration::from_swatches(&few, 244.0, (0.0, 0.0), None, None).is_err());
    }
}
