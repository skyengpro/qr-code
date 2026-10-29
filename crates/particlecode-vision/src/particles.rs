//! Reading the six grey particles in the outer band.

use particlecode_core::layout::{self, ParticleBand, dims};

use crate::homography::Homography;
use crate::image::PlaneRef;

/// Sector (`0..16`) of each particle, or `None` if any arc has no clear peak.
///
/// The band is sampled on a polar grid through `h`; ink (`bg − luma`) is summed across the
/// band's width into an angular profile, smoothed by about a particle radius, and each
/// particle's 60° arc contributes the angle of its maximum. A particle is accepted when its
/// peak stands well above the arc's other local maxima and is comparable to the other
/// particles' peaks.
pub(crate) fn read_particles(
    luma: PlaneRef<'_>,
    h: &Homography,
    band: ParticleBand,
    bg: f64,
) -> Option<[u8; dims::PARTICLE_COUNT]> {
    let mid = band.mid();
    // 0.2 pitch of arc per bin.
    let bins = ((std::f64::consts::TAU * mid / 0.2).ceil() as usize).clamp(360, 4096);
    let dtheta = std::f64::consts::TAU / bins as f64;
    let (r0, r1) = (band.inner + 0.5, band.outer - 0.5);
    if r1 <= r0 {
        return None;
    }
    let radial = 5usize;
    let dead = 0.08 * bg;
    let mut profile = vec![0.0f64; bins];
    let mut valid = vec![true; bins];
    for (k, slot) in profile.iter_mut().enumerate() {
        let a = (k as f64 + 0.5) * dtheta - std::f64::consts::PI;
        let (s, c) = a.sin_cos();
        let mut acc = 0.0;
        for i in 0..radial {
            let r = r0 + (r1 - r0) * i as f64 / (radial - 1) as f64;
            match h.apply((r * c, r * s)) {
                Some((x, y)) if luma.contains(x, y, 1.0) => {
                    acc += (bg - f64::from(luma.bilinear(x, y)) - dead).max(0.0);
                }
                _ => valid[k] = false,
            }
        }
        *slot = acc;
    }
    // Smooth with a Gaussian of about one particle radius along the arc.
    let sigma_bins = (dims::PARTICLE_R / (mid * dtheta)).max(1.0);
    let smooth = circular_gaussian(&profile, sigma_bins);

    let mut peaks = [0.0f64; dims::PARTICLE_COUNT];
    let mut angles = [0.0f64; dims::PARTICLE_COUNT];
    for k in 0..dims::PARTICLE_COUNT {
        let start = layout::particle_arc_start(k);
        let end = start + std::f64::consts::TAU / dims::PARTICLE_COUNT as f64;
        let mut best: Option<(usize, f64)> = None;
        let bin_of = |a: f64| -> usize {
            let rel = (a + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU);
            ((rel / dtheta) as usize).min(bins - 1)
        };
        let (b0, b1) = (bin_of(start), bin_of(end));
        let count = (b1 + bins - b0) % bins;
        for i in 0..count {
            let b = (b0 + i) % bins;
            if !valid[b] {
                return None;
            }
            if best.is_none_or(|(_, v)| smooth[b] > v) {
                best = Some((b, smooth[b]));
            }
        }
        let (b, v) = best?;
        // The strongest competitor outside the peak's neighbourhood.
        let guard = (3.0 * sigma_bins).ceil() as usize;
        let mut rival = 0.0f64;
        for i in 0..count {
            let bb = (b0 + i) % bins;
            let d = bb.abs_diff(b).min(bins - bb.abs_diff(b));
            if d > guard {
                rival = rival.max(smooth[bb]);
            }
        }
        if v <= 0.0 || rival > 0.5 * v {
            return None;
        }
        peaks[k] = v;
        angles[k] = (b as f64 + 0.5) * dtheta - std::f64::consts::PI;
    }
    let mut sorted = peaks;
    sorted.sort_by(f64::total_cmp);
    let median = 0.5 * (sorted[2] + sorted[3]);
    if peaks.iter().any(|&p| p < 0.3 * median) {
        return None;
    }
    let mut out = [0u8; dims::PARTICLE_COUNT];
    for k in 0..dims::PARTICLE_COUNT {
        out[k] = layout::particle_sector(k, angles[k])?;
    }
    Some(out)
}

fn circular_gaussian(v: &[f64], sigma: f64) -> Vec<f64> {
    let n = v.len();
    let rad = (3.0 * sigma).ceil() as i64;
    let kernel: Vec<f64> = (-rad..=rad)
        .map(|i| (-(i * i) as f64 / (2.0 * sigma * sigma)).exp())
        .collect();
    let norm: f64 = kernel.iter().sum();
    (0..n)
        .map(|i| {
            kernel
                .iter()
                .enumerate()
                .map(|(j, k)| {
                    let idx = (i as i64 + j as i64 - rad).rem_euclid(n as i64) as usize;
                    k * v[idx]
                })
                .sum::<f64>()
                / norm
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaussian_preserves_a_spike_position() {
        let mut v = vec![0.0; 100];
        v[40] = 10.0;
        let s = circular_gaussian(&v, 2.0);
        let argmax = s
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|p| p.0);
        assert_eq!(argmax, Some(40));
    }
}
