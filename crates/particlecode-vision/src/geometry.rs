//! Homography refinement against the anchors, the center mark, the swatches and the header
//! dots: everything the halos offer before the dot order is known.

use particlecode_core::Profile;
use particlecode_core::layout::{self, SwatchColor, dims};

use crate::anchor::refine_dark_blob;
use crate::error::VisionError;
use crate::homography::{Homography, fit_affine, fit_homography};
use crate::image::PlaneRef;
use crate::sample::ink_centroid;
use crate::util::median;

/// Smallest usable scale in image pixels per dot pitch: below it the dots are a few pixels
/// wide and neither locking nor reading is attempted.
pub(crate) const MIN_PITCH_PX: f64 = 3.0;
/// RMS residual (pitch units) above which a lock is rejected.
pub(crate) const MAX_RESIDUAL_PITCH: f64 = 0.15;

/// A list of 2-D points.
type Points = Vec<(f64, f64)>;

/// A correspondence between a frame point and where it was seen.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Pair {
    pub(crate) src: (f64, f64),
    pub(crate) dst: (f64, f64),
    /// Repetitions in the least-squares fit.
    pub(crate) weight: usize,
}

/// Swatch and header-dot positions of all three halos (the white swatch has no ink).
pub(crate) fn halo_points(profile: Profile) -> Vec<(f64, f64)> {
    let mut pts = Vec::with_capacity(96);
    for a in 0..3 {
        for s in 0..dims::HALO_SLOTS {
            let sw = layout::swatch(profile, a, s);
            if sw.color != SwatchColor::White {
                pts.push((sw.at.x, sw.at.y));
            }
        }
    }
    for j in 0..layout::HEADER_DOTS {
        let p = layout::header_dot(profile, j);
        pts.push((p.x, p.y));
    }
    pts
}

/// Ink centroid of a dot-sized blob near `pred`; `None` if it is not there.
fn locate_dot(
    luma: PlaneRef<'_>,
    h: &Homography,
    bg: f64,
    pt: (f64, f64),
    max_move: f64,
) -> Option<(f64, f64)> {
    let pred = h.apply(pt)?;
    let p = h.scale_at(pt)?;
    let ic = ink_centroid(luma, pred, 0.62 * p, 0.32 * p, bg, 3)?;
    // A dot has at least a fifth of a small dot's ink in the window.
    let min_mass = 0.2 * 0.4 * bg * 0.45 * p * p;
    let moved = (ic.at.0 - pred.0).hypot(ic.at.1 - pred.1);
    (ic.mass >= min_mass && moved <= max_move * p).then_some(ic.at)
}

/// Fit with trimming of outliers. Returns the homography and the RMS residual in pitch
/// units over the kept pairs, plus the kept indices.
pub(crate) fn fit_trimmed(
    pairs: &[Pair],
    profile: Profile,
) -> Option<(Homography, f64, Vec<usize>)> {
    let expand = |idx: &[usize]| -> (Points, Points) {
        let mut s = Vec::new();
        let mut d = Vec::new();
        for &i in idx {
            for _ in 0..pairs[i].weight {
                s.push(pairs[i].src);
                d.push(pairs[i].dst);
            }
        }
        (s, d)
    };
    let fit = |idx: &[usize]| -> Option<Homography> {
        let (s, d) = expand(idx);
        let h = fit_homography(&s, &d)?;
        if plausible(&h, profile) {
            Some(h)
        } else {
            fit_affine(&s, &d)
        }
    };
    let residual = |h: &Homography, i: usize| -> f64 {
        let Some(q) = h.apply(pairs[i].src) else {
            return f64::MAX;
        };
        let scale = h.scale_at(pairs[i].src).unwrap_or(1.0).max(1e-6);
        (q.0 - pairs[i].dst.0).hypot(q.1 - pairs[i].dst.1) / scale
    };
    let all: Vec<usize> = (0..pairs.len()).collect();
    let mut h = fit(&all)?;
    let mut kept = all;
    for _ in 0..2 {
        let mut res: Vec<f64> = kept.iter().map(|&i| residual(&h, i)).collect();
        let med = median(&mut res);
        let limit = (3.0 * med).clamp(0.2, 0.5);
        let next: Vec<usize> = (0..pairs.len())
            .filter(|&i| residual(&h, i) <= limit)
            .collect();
        if next.len() < 8 {
            break;
        }
        kept = next;
        h = fit(&kept)?;
    }
    let sq: f64 = kept.iter().map(|&i| residual(&h, i).powi(2)).sum();
    Some((h, (sq / kept.len() as f64).sqrt(), kept))
}

/// A 4-point-style homography is plausible if the local area scale stays within a
/// factor of three of the center's across the field.
fn plausible(h: &Homography, profile: Profile) -> bool {
    let f = profile.field_half();
    let Some(s0) = h.area_scale((0.0, 0.0)) else {
        return false;
    };
    [(-f, -f), (f, -f), (f, f), (-f, f)].iter().all(|&q| {
        h.area_scale(q)
            .is_some_and(|s| s / s0 > 0.35 && s / s0 < 2.9)
    })
}

/// A refined homography with the observations behind it.
pub(crate) struct Geometry {
    pub(crate) h: Homography,
    pub(crate) pairs: Vec<Pair>,
    pub(crate) residual_pitch: f64,
    pub(crate) residual_px: f64,
}

/// Observed positions of the anchors, the center mark, the swatches and the header dots,
/// found by searching around where `h` predicts them. Also returns how many halo points
/// fall inside the image.
pub(crate) fn collect_halo_pairs(
    luma: PlaneRef<'_>,
    h: &Homography,
    bg_luma: f64,
    profile: Profile,
) -> Result<(Vec<Pair>, usize), VisionError> {
    let halo = halo_points(profile);
    let mut pairs: Vec<Pair> = Vec::with_capacity(halo.len() + 4);
    for a in &layout::hero_anchors(profile) {
        let src = (a.x, a.y);
        let (Some(pred), Some(p)) = (h.apply(src), h.scale_at(src)) else {
            return Err(VisionError::NoLock);
        };
        if let Some(blob) = refine_dark_blob(luma, pred, (dims::HERO_RING_OUT + 0.15) * p, 50.0) {
            pairs.push(Pair {
                src,
                dst: blob.at,
                weight: 3,
            });
        }
    }
    if pairs.len() < 3 {
        return Err(if halo_visible(h, luma, &halo) {
            VisionError::NoLock
        } else {
            VisionError::Truncated
        });
    }
    // Center anchor: ink centroid inside the keep-out, where only the mark lives.
    if let (Some(pred), Some(p)) = (h.apply((0.0, 0.0)), h.scale_at((0.0, 0.0)))
        && let Some(ic) = ink_centroid(luma, pred, 1.2 * p, 0.7 * p, bg_luma, 3)
    {
        let expect = 2.4 * p * p * 0.55 * bg_luma;
        let near = (ic.at.0 - pred.0).hypot(ic.at.1 - pred.1) < 0.6 * p;
        if ic.mass > 0.4 * expect && ic.mass < 2.0 * expect && near {
            pairs.push(Pair {
                src: (0.0, 0.0),
                dst: ic.at,
                weight: 2,
            });
        }
    }
    let mut visible = 0usize;
    for &pt in &halo {
        let (Some(pred), Some(p)) = (h.apply(pt), h.scale_at(pt)) else {
            continue;
        };
        if luma.contains(pred.0, pred.1, 0.62 * p + 3.0) {
            visible += 1;
        }
        if let Some(dst) = locate_dot(luma, h, bg_luma, pt, 0.4) {
            pairs.push(Pair {
                src: pt,
                dst,
                weight: 1,
            });
        }
    }
    Ok((pairs, visible))
}

pub(crate) fn refine_geometry(
    luma: PlaneRef<'_>,
    profile: Profile,
    start: Homography,
    bg_luma: f64,
) -> Result<Geometry, VisionError> {
    let n_halo = halo_points(profile).len();
    let mut h = start;
    let mut best: Option<Geometry> = None;
    for _ in 0..3 {
        let (pairs, visible) = collect_halo_pairs(luma, &h, bg_luma, profile)?;
        if visible * 10 < n_halo * 6 {
            return Err(VisionError::Truncated);
        }
        if pairs.len() < 20 {
            return Err(VisionError::NoLock);
        }
        let Some((nh, rms, kept)) = fit_trimmed(&pairs, profile) else {
            return Err(VisionError::NoLock);
        };
        if kept.len() * 10 < pairs.len() * 6 {
            return Err(VisionError::Residual);
        }
        h = nh;
        let scale = h.scale_at((0.0, 0.0)).unwrap_or(1.0);
        let kept_pairs: Vec<Pair> = kept.iter().map(|&i| pairs[i]).collect();
        best = Some(Geometry {
            h,
            pairs: kept_pairs,
            residual_pitch: rms,
            residual_px: rms * scale,
        });
    }
    let geo = best.ok_or(VisionError::NoLock)?;
    if geo.residual_pitch > MAX_RESIDUAL_PITCH {
        return Err(VisionError::Residual);
    }
    Ok(geo)
}

fn halo_visible(h: &Homography, luma: PlaneRef<'_>, halo: &[(f64, f64)]) -> bool {
    let n = halo
        .iter()
        .filter(|&&pt| h.apply(pt).is_some_and(|q| luma.contains(q.0, q.1, 4.0)))
        .count();
    n * 10 >= halo.len() * 6
}

/// Largest displacement (pitch units) between two homographies over the field corners and
/// center.
pub(crate) fn displacement(a: &Homography, b: &Homography, profile: Profile) -> f64 {
    let f = 0.9 * profile.field_half();
    [(0.0, 0.0), (f, f), (-f, f), (f, -f), (-f, -f)]
        .iter()
        .map(|&p| match (a.apply(p), b.apply(p), a.scale_at(p)) {
            (Some(u), Some(v), Some(s)) => (u.0 - v.0).hypot(u.1 - v.1) / s,
            _ => f64::MAX,
        })
        .fold(0.0, f64::max)
}

/// RMS pitch-unit residual of `h` over the halo pairs.
pub(crate) fn halo_rms(h: &Homography, pairs: &[Pair]) -> f64 {
    let mut sum = 0.0;
    let mut n = 0.0;
    for p in pairs {
        if let (Some(q), Some(s)) = (h.apply(p.src), h.scale_at(p.src)) {
            sum += ((q.0 - p.dst.0).hypot(q.1 - p.dst.1) / s).powi(2);
            n += 1.0;
        }
    }
    if n > 0.0 { (sum / n).sqrt() } else { f64::MAX }
}
