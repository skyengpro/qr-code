//! Anchor detection and orientation: `locate`.
//!
//! 1. Adaptive threshold (`pixel < α · local mean`, integral image) isolates the very dark
//!    anchors from the dots and the background.
//! 2. Run-based 8-connected labelling; ring-with-core pairs (and, if blur merged them,
//!    solid discs) become anchor candidates.
//! 3. Triples of candidates with equal sides and equal ring size are tested against every
//!    (profile, rotation) hypothesis by looking at the calibration swatches of the halos: the
//!    top halo has a white and a black swatch in known places.
//! 4. The winning hypothesis is refined at full resolution (anchors, center mark, then the
//!    swatches and header dots) and must pass a stricter test of the same swatches, plus a
//!    permutation test against displaced halos, to become a [`Lock`].

use particlecode_core::Profile;
use particlecode_core::layout::{self, SwatchColor, dims};

use crate::anchor::refine_dark_blob;
use crate::geometry::{MIN_PITCH_PX, refine_geometry};
use crate::homography::{Homography, fit_affine, fit_homography};
use crate::image::PlaneRef;
use crate::sample::{PaperPlane, fit_paper_plane};
use crate::util::{disc_mean, median};

/// Result of locating a Particle Code: which profile it is, where its anchors are and the
/// map from frame coordinates to image pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lock {
    /// Profile implied by the anchor geometry (confirmed by the header later).
    pub profile: Profile,
    /// Frame coordinates (pitch units, origin at the frame center, y down) to image pixels.
    pub homography: Homography,
    /// Hero anchor centers in image pixels, in layout order: top (−90°), lower right
    /// (30°), lower left (150°).
    pub anchors_px: [(f64, f64); 3],
    /// Center anchor, if it was found.
    pub center_px: Option<(f64, f64)>,
    /// Local scale at the frame center: image pixels per dot pitch.
    pub pitch_px: f64,
    /// Orientation test score in `0..=1` (white/black swatch contrast); higher is surer.
    pub score: f64,
}

/// Thresholds `α` (percent of the local mean) tried in order.
const ALPHAS_PCT: [u64; 4] = [40, 30, 50, 62];
/// Maximum number of components considered per threshold.
const MAX_COMPONENTS: usize = 800;
/// Maximum number of anchor candidates kept.
const MAX_CANDIDATES: usize = 48;
/// Up to this many candidates the picture counts as calm and weak (merged) candidates count.
const MAX_CALM_CANDIDATES: usize = 16;
/// Maximum number of triples examined per threshold.
const MAX_TRIPLES: usize = 24;

/// Find a Particle Code in a luma plane.
///
/// Large images are searched coarse to fine: on a 4× (from 2000 px) and a 2× (from 900 px)
/// box-filtered copy first, where the anchors are still obvious and a frame costs a fraction,
/// then at full resolution. Returns `None` when no triple of anchors with a consistent halo is
/// found. Mirrored views lock like any other (the halo is mirror-symmetric); `read` notices.
pub fn locate<'a>(luma: impl Into<PlaneRef<'a>>) -> Option<Lock> {
    let full: PlaneRef<'_> = luma.into();
    let short = full.width().min(full.height());
    if short < 48 {
        return None;
    }
    // Box-filtered copies, finest first: (scale, width, height, pixels).
    let mut levels: Vec<(usize, usize, usize, Vec<u8>)> = Vec::new();
    if short >= 900 {
        let (w, h, d) = full.downscale2()?;
        levels.push((2, w, h, d));
        if short >= 2000 {
            let (w, h, d) = PlaneRef::new(w, h, w, &levels[0].3)?.downscale2()?;
            levels.push((4, w, h, d));
        }
    }
    for (scale, w, h, data) in levels.iter().rev() {
        if let Some(lock) = PlaneRef::new(*w, *h, *w, data).and_then(|p| detect_at(p, *scale, full))
        {
            return Some(lock);
        }
    }
    detect_at(full, 1, full)
}

fn detect_at(img: PlaneRef<'_>, scale: usize, full: PlaneRef<'_>) -> Option<Lock> {
    if img.width() < 32 || img.height() < 32 {
        return None;
    }
    let integral = Integral::new(img)?;
    let k = (img.width().min(img.height()) / 16).max(6);
    for alpha in ALPHAS_PCT {
        let Some(comps) = components(img, &integral, k, alpha) else {
            continue;
        };
        let cands = candidates(&comps, img.width(), img.height(), scale);
        if cands.len() < 3 {
            continue;
        }
        if let Some(lock) = try_triples(full, &cands) {
            return Some(lock);
        }
    }
    None
}

// ---------------------------------------------------------------------------------------
// Integral image and thresholded components
// ---------------------------------------------------------------------------------------

struct Integral {
    w: usize,
    data: Vec<u32>,
}

impl Integral {
    fn new(img: PlaneRef<'_>) -> Option<Integral> {
        let (w, h) = (img.width(), img.height());
        // 255 · pixels must fit u32.
        if w.checked_mul(h)? > 16_000_000 {
            return None;
        }
        let iw = w + 1;
        let mut data = vec![0u32; iw * (h + 1)];
        for y in 0..h {
            let row = img.row(y);
            let mut run = 0u32;
            for x in 0..w {
                run += u32::from(row[x]);
                data[(y + 1) * iw + x + 1] = data[y * iw + x + 1] + run;
            }
        }
        Some(Integral { w: iw, data })
    }

    #[inline]
    fn sum(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> u32 {
        let w = self.w;
        self.data[y1 * w + x1] + self.data[y0 * w + x0]
            - self.data[y0 * w + x1]
            - self.data[y1 * w + x0]
    }
}

/// A connected component of the dark mask.
#[derive(Debug, Clone, Copy)]
struct Comp {
    area: u32,
    sx: f64,
    sy: f64,
    min_x: u32,
    max_x: u32,
    min_y: u32,
    max_y: u32,
}

impl Comp {
    fn cx(&self) -> f64 {
        self.sx / f64::from(self.area)
    }
    fn cy(&self) -> f64 {
        self.sy / f64::from(self.area)
    }
    fn bw(&self) -> f64 {
        f64::from(self.max_x - self.min_x + 1)
    }
    fn bh(&self) -> f64 {
        f64::from(self.max_y - self.min_y + 1)
    }
}

#[derive(Clone, Copy)]
struct Run {
    x0: u32,
    x1: u32,
    y: u32,
}

fn find(parent: &mut [u32], mut i: u32) -> u32 {
    while parent[i as usize] != i {
        let gp = parent[parent[i as usize] as usize];
        parent[i as usize] = gp;
        i = gp;
    }
    i
}

fn union(parent: &mut [u32], a: u32, b: u32) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
        parent[hi as usize] = lo;
    }
}

/// Label the pixels with `pixel · 100 < alpha_pct · local_mean` (window half-size `k`).
fn components(
    img: PlaneRef<'_>,
    integral: &Integral,
    k: usize,
    alpha_pct: u64,
) -> Option<Vec<Comp>> {
    let (w, h) = (img.width(), img.height());
    let mut runs: Vec<Run> = Vec::new();
    let mut parent: Vec<u32> = Vec::new();
    let mut prev = 0..0usize;
    for y in 0..h {
        let (y0, y1) = (y.saturating_sub(k), (y + k + 1).min(h));
        let row = img.row(y);
        let start = runs.len();
        let mut open: Option<u32> = None;
        for (x, &pixel) in row.iter().enumerate() {
            let (x0, x1) = (x.saturating_sub(k), (x + k + 1).min(w));
            let count = ((x1 - x0) * (y1 - y0)) as u64;
            let sum = u64::from(integral.sum(x0, y0, x1, y1));
            let dark = u64::from(pixel) * count * 100 < alpha_pct * sum;
            match (dark, open) {
                (true, None) => open = Some(x as u32),
                (false, Some(s)) => {
                    runs.push(Run {
                        x0: s,
                        x1: x as u32 - 1,
                        y: y as u32,
                    });
                    open = None;
                }
                _ => {}
            }
        }
        if let Some(s) = open {
            runs.push(Run {
                x0: s,
                x1: w as u32 - 1,
                y: y as u32,
            });
        }
        if runs.len() > 300_000 {
            return None;
        }
        let cur = start..runs.len();
        parent.extend(cur.clone().map(|i| i as u32));
        // 8-connectivity: runs overlap if their x-ranges, widened by one, intersect.
        let mut pi = prev.start;
        for ci in cur.clone() {
            let c = runs[ci];
            while pi < prev.end && runs[pi].x1 + 1 < c.x0 {
                pi += 1;
            }
            let mut pj = pi;
            while pj < prev.end && runs[pj].x0 <= c.x1 + 1 {
                union(&mut parent, ci as u32, pj as u32);
                pj += 1;
            }
        }
        prev = cur;
    }
    let mut index: Vec<u32> = vec![u32::MAX; runs.len()];
    let mut comps: Vec<Comp> = Vec::new();
    for (i, &r) in runs.iter().enumerate() {
        let root = find(&mut parent, i as u32) as usize;
        let len = f64::from(r.x1 - r.x0 + 1);
        let slot = if index[root] == u32::MAX {
            index[root] = comps.len() as u32;
            comps.push(Comp {
                area: 0,
                sx: 0.0,
                sy: 0.0,
                min_x: u32::MAX,
                max_x: 0,
                min_y: u32::MAX,
                max_y: 0,
            });
            comps.len() - 1
        } else {
            index[root] as usize
        };
        let c = &mut comps[slot];
        c.area += r.x1 - r.x0 + 1;
        c.sx += f64::from(r.x0 + r.x1) * 0.5 * len;
        c.sy += f64::from(r.y) * len;
        c.min_x = c.min_x.min(r.x0);
        c.max_x = c.max_x.max(r.x1);
        c.min_y = c.min_y.min(r.y);
        c.max_y = c.max_y.max(r.y);
    }
    Some(comps)
}

// ---------------------------------------------------------------------------------------
// Anchor candidates
// ---------------------------------------------------------------------------------------

/// A possible hero anchor, in full-resolution pixels.
#[derive(Debug, Clone, Copy)]
struct Cand {
    x: f64,
    y: f64,
    /// Outer radius estimate, pixels.
    r: f64,
    /// Ring with a concentric core found (as opposed to a merged solid disc).
    strong: bool,
}

fn candidates(comps: &[Comp], w: usize, h: usize, scale: usize) -> Vec<Cand> {
    let max_area = (w * h) as f64 * 0.05;
    let mut usable: Vec<&Comp> = comps
        .iter()
        .filter(|c| {
            let (bw, bh) = (c.bw(), c.bh());
            f64::from(c.area) >= 6.0
                && f64::from(c.area) <= max_area
                && bw >= 3.0
                && bh >= 3.0
                && bw / bh < 2.2
                && bh / bw < 2.2
        })
        .collect();
    if usable.len() > MAX_COMPONENTS {
        usable.sort_by_key(|c| std::cmp::Reverse(c.area));
        usable.truncate(MAX_COMPONENTS);
    }
    let to_full = |v: f64| v * scale as f64 + (scale as f64 - 1.0) / 2.0;
    let mut out: Vec<Cand> = Vec::new();
    for ring in &usable {
        let (bw, bh) = (ring.bw(), ring.bh());
        let r_out = (bw + bh) / 4.0;
        if r_out < 3.0 {
            continue;
        }
        let fill = f64::from(ring.area) / (std::f64::consts::PI * r_out * r_out);
        let (rx, ry) = (ring.cx(), ring.cy());
        if (0.28..=0.80).contains(&fill) {
            // Ring-like: look for a concentric core.
            let core = usable.iter().any(|k| {
                !std::ptr::eq(*k, *ring)
                    && (k.cx() - rx).hypot(k.cy() - ry) <= 0.3 * r_out
                    && k.bw().max(k.bh()) <= 0.7 * bw.min(bh)
                    && f64::from(k.area) * 2.5 <= f64::from(ring.area)
                    && f64::from(k.area) * 16.0 >= f64::from(ring.area)
            });
            out.push(Cand {
                x: to_full(rx),
                y: to_full(ry),
                r: r_out * scale as f64,
                strong: core,
            });
        } else if fill > 0.80 && f64::from(ring.area) >= 30.0 {
            // Solid disc: a ring and core merged by blur.
            out.push(Cand {
                x: to_full(rx),
                y: to_full(ry),
                r: r_out * scale as f64,
                strong: false,
            });
        }
    }
    // Strong first, then larger.
    out.sort_by(|a, b| b.strong.cmp(&a.strong).then(b.r.total_cmp(&a.r)));
    out.truncate(MAX_CANDIDATES);
    out
}

// ---------------------------------------------------------------------------------------
// Triples and hypotheses
// ---------------------------------------------------------------------------------------

struct Triple {
    idx: [usize; 3],
    cost: f64,
}

fn try_triples(full: PlaneRef<'_>, cands: &[Cand]) -> Option<Lock> {
    // A ring merged with its core by blur is only a dark disc, and dark discs are everywhere
    // in a busy or noisy picture. Trust such weak candidates only when the picture is calm.
    let allow_weak = cands.len() <= MAX_CALM_CANDIDATES;
    let mut triples: Vec<Triple> = Vec::new();
    for i in 0..cands.len() {
        for j in i + 1..cands.len() {
            for k in j + 1..cands.len() {
                if let Some(cost) = triple_cost(&cands[i], &cands[j], &cands[k], allow_weak) {
                    triples.push(Triple {
                        idx: [i, j, k],
                        cost,
                    });
                }
            }
        }
    }
    triples.sort_by(|a, b| a.cost.total_cmp(&b.cost));
    triples.truncate(MAX_TRIPLES);
    for t in &triples {
        let c = [cands[t.idx[0]], cands[t.idx[1]], cands[t.idx[2]]];
        if let Some(lock) = best_hypothesis(full, &c) {
            return Some(lock);
        }
    }
    None
}

fn triple_cost(a: &Cand, b: &Cand, c: &Cand, allow_weak: bool) -> Option<f64> {
    let d = |p: &Cand, q: &Cand| (p.x - q.x).hypot(p.y - q.y);
    let sides = [d(a, b), d(b, c), d(a, c)];
    let (smin, smax) = (
        sides.iter().copied().fold(f64::MAX, f64::min),
        sides.iter().copied().fold(0.0, f64::max),
    );
    let (rmin, rmax) = (a.r.min(b.r).min(c.r), a.r.max(b.r).max(c.r));
    if smin < 1.0 || rmin < 1.0 {
        return None;
    }
    let side_spread = smax / smin;
    let radius_spread = rmax / rmin;
    let ratio = (sides[0] + sides[1] + sides[2]) / 3.0 / ((a.r + b.r + c.r) / 3.0);
    if side_spread > 1.25 || radius_spread > 1.45 || !(9.0..=48.0).contains(&ratio) {
        return None;
    }
    let weak = [a, b, c].iter().filter(|x| !x.strong).count();
    if weak > 0 && !allow_weak {
        return None;
    }
    let weak = weak as f64;
    Some((side_spread - 1.0) + 0.5 * (radius_spread - 1.0) + 0.12 * weak)
}

/// All 6 assignments of the three layout anchors to the three image candidates.
const PERMS: [[usize; 3]; 6] = [
    [0, 1, 2],
    [0, 2, 1],
    [1, 0, 2],
    [1, 2, 0],
    [2, 0, 1],
    [2, 1, 0],
];

fn best_hypothesis(full: PlaneRef<'_>, cands: &[Cand; 3]) -> Option<Lock> {
    let mean_r = (cands[0].r + cands[1].r + cands[2].r) / 3.0;
    let mut best: Option<(f64, Profile, [usize; 3], Homography)> = None;
    for profile in Profile::ALL {
        for perm in PERMS {
            let pts = [
                (cands[perm[0]].x, cands[perm[0]].y),
                (cands[perm[1]].x, cands[perm[1]].y),
                (cands[perm[2]].x, cands[perm[2]].y),
            ];
            if let Some((score, h)) = score_hypothesis(full, profile, &pts, mean_r)
                && best.as_ref().is_none_or(|b| score > b.0)
            {
                best = Some((score, profile, perm, h));
            }
        }
    }
    let (score, profile, perm, h0) = best?;
    let pts = [
        (cands[perm[0]].x, cands[perm[0]].y),
        (cands[perm[1]].x, cands[perm[1]].y),
        (cands[perm[2]].x, cands[perm[2]].y),
    ];
    finalize(full, profile, &pts, &h0, score)
}

/// Test one (profile, assignment) hypothesis; `Some((score, affine homography))` if the
/// halo of the top anchor shows the expected white and black swatches.
fn score_hypothesis(
    luma: PlaneRef<'_>,
    profile: Profile,
    pts: &[(f64, f64); 3],
    mean_ring_r: f64,
) -> Option<(f64, Homography)> {
    let anchors = layout::hero_anchors(profile);
    let src = [
        (anchors[0].x, anchors[0].y),
        (anchors[1].x, anchors[1].y),
        (anchors[2].x, anchors[2].y),
    ];
    let h = fit_affine(&src, pts)?;
    // The white/black test is blind to a mirror about the vertical axis, and luma cannot
    // tell the hue order apart. Camera images of a screen keep the handedness of the frame
    // (both are y-down), so only orientation-preserving maps are considered.
    if h.0[0][0] * h.0[1][1] - h.0[0][1] * h.0[1][0] <= 0.0 {
        return None;
    }
    let p = h.scale_at((0.0, 0.0))?;
    // The ring's outer radius pins the pitch independently of the triangle size.
    let expect_r = dims::HERO_RING_OUT * p;
    if !(0.5..=1.35).contains(&(mean_ring_r / expect_r)) {
        return None;
    }
    let shading = shading_for(luma, profile, &h, p);
    let stats = halo_stats(luma, profile, &h, p, true, shading.as_ref())?;
    stats.passes(false).then(|| (stats.score(), h))
}

/// The lighting gradient across the code, if it is worth correcting for (the paper level
/// varies by more than 3 % across the field).
fn shading_for(luma: PlaneRef<'_>, profile: Profile, h: &Homography, p: f64) -> Option<PaperPlane> {
    let f = profile.field_half();
    fit_paper_plane(luma, h, (1.07 * f, 1.18 * f), 0.85)
        .filter(|pl| pl.variation(2.6 * f * p) >= 0.03)
}

/// Number of the six one-pitch displacements of the halos that still pass the loose
/// swatch test. A real code has none; noise and texture pass them as often as the original.
fn shifted_passes(
    luma: PlaneRef<'_>,
    profile: Profile,
    h: &Homography,
    p: f64,
    shading: Option<&PaperPlane>,
) -> usize {
    (0..6)
        .filter(|k| {
            let a = f64::from(*k) * std::f64::consts::FRAC_PI_3;
            let shift = Homography([[1.0, 0.0, a.cos()], [0.0, 1.0, a.sin()], [0.0, 0.0, 1.0]]);
            let shifted = h.then_after(&shift);
            halo_stats(luma, profile, &shifted, p, false, shading).is_some_and(|s| s.passes(false))
        })
        .count()
}

/// What the calibration swatches look like through a candidate homography.
#[derive(Debug, Clone, Copy)]
struct HaloStats {
    /// Luma at the white swatch of the top halo.
    white: f64,
    /// Luma at the black swatch of the top halo.
    black: f64,
    /// Median luma over the 46 hue swatches.
    hue_median: f64,
    /// Fraction of hue swatches that look inked.
    inked: f64,
    /// Fraction of header dots that look inked (they always carry a dot, whatever it says).
    header_inked: f64,
}

impl HaloStats {
    /// Gates on the statistics. The loose set tolerates a coarse (affine) homography under
    /// perspective; the strict set is applied to the refined homography.
    fn passes(&self, strict: bool) -> bool {
        let (min_inked, white_ratio, black_ratio) = if strict {
            (0.8, 1.3, 0.75)
        } else {
            (0.7, 1.25, 0.8)
        };
        self.hue_median >= 1.0
            && self.hue_median <= 0.8 * self.white
            && self.white >= white_ratio * self.hue_median
            && self.black <= black_ratio * self.hue_median
            && self.black <= 0.6 * self.white
            && self.inked >= min_inked
            && (!strict || self.header_inked >= 0.6)
    }

    fn score(&self) -> f64 {
        ((self.white - self.hue_median) / self.white)
            * ((self.hue_median - self.black) / self.hue_median)
    }
}

/// Sample the swatches and header dots of all three halos through `h` (pitch `p` pixels).
///
/// A coarse prediction is only exact at the anchors; under perspective the swatches can be a
/// fifth of a pitch or more off. With `tolerant` each position is probed at five places and
/// the darkest (ink) or brightest (white) reading is kept: a forgiving test for candidate
/// hypotheses. Without it the reading is the mean luma of a single disc of 0.28 pitch, the
/// test for a refined homography — probing for the darkest of several positions would let
/// noise and texture pass.
fn halo_stats(
    luma: PlaneRef<'_>,
    profile: Profile,
    h: &Homography,
    p: f64,
    tolerant: bool,
    shading: Option<&PaperPlane>,
) -> Option<HaloStats> {
    let (radius, off) = if tolerant {
        (0.15 * p, 0.18 * p)
    } else {
        (0.28 * p, 0.0)
    };
    let cross = [(0.0, 0.0), (off, 0.0), (-off, 0.0), (0.0, off), (0.0, -off)];
    let places = if tolerant { 5 } else { 1 };
    let read = |pt: (f64, f64), brightest: bool| -> f64 {
        let gain = shading.map_or(1.0, |sh| sh.gain(pt.0, pt.1));
        let vals = cross[..places]
            .iter()
            .map(|d| gain * disc_mean(luma, (pt.0 + d.0, pt.1 + d.1), radius));
        if brightest {
            vals.fold(f64::MIN, f64::max)
        } else {
            vals.fold(f64::MAX, f64::min)
        }
    };
    let mut white = None;
    let mut black = None;
    let mut hues: Vec<f64> = Vec::with_capacity(48);
    for a in 0..3 {
        for s in 0..dims::HALO_SLOTS {
            let sw = layout::swatch(profile, a, s);
            let pt = h.apply((sw.at.x, sw.at.y))?;
            if !luma.contains(pt.0, pt.1, 2.0) {
                continue;
            }
            match sw.color {
                SwatchColor::White => white = Some(read(pt, true)),
                SwatchColor::Black => black = Some(read(pt, false)),
                SwatchColor::Hue(_) => hues.push(read(pt, false)),
            }
        }
    }
    let (white, black) = (white?, black?);
    if hues.len() < 30 {
        return None;
    }
    let hue_median = median(&mut hues.clone());
    let ink_below = 0.75 * white;
    let inked = hues.iter().filter(|&&l| l < ink_below).count() as f64 / hues.len() as f64;
    let mut header_total = 0usize;
    let mut header_inked = 0usize;
    for j in 0..layout::HEADER_DOTS {
        let d = layout::header_dot(profile, j);
        let pt = h.apply((d.x, d.y))?;
        if luma.contains(pt.0, pt.1, 2.0) {
            header_total += 1;
            header_inked += usize::from(read(pt, false) < 0.88 * white);
        }
    }
    Some(HaloStats {
        white,
        black,
        hue_median,
        inked,
        header_inked: if header_total == 0 {
            0.0
        } else {
            header_inked as f64 / header_total as f64
        },
    })
}

/// Refine the anchors at full resolution and build the final lock.
fn finalize(
    luma: PlaneRef<'_>,
    profile: Profile,
    pts: &[(f64, f64); 3],
    h0: &Homography,
    score: f64,
) -> Option<Lock> {
    let p0 = h0.scale_at((0.0, 0.0))?;
    let anchors = layout::hero_anchors(profile);
    let mut refined = [(0.0, 0.0); 3];
    for (i, a) in anchors.iter().enumerate() {
        let pred = h0.apply((a.x, a.y)).unwrap_or(pts[i]);
        let blob = refine_dark_blob(luma, pred, (dims::HERO_RING_OUT + 0.15) * p0, 50.0)?;
        refined[i] = blob.at;
    }
    let src = [
        (anchors[0].x, anchors[0].y),
        (anchors[1].x, anchors[1].y),
        (anchors[2].x, anchors[2].y),
    ];
    let affine = fit_affine(&src, &refined)?;
    let center = find_center(luma, &affine, p0);
    let homography = match center {
        Some(c) => {
            let mut s = src.to_vec();
            let mut d = refined.to_vec();
            s.push((0.0, 0.0));
            d.push(c);
            match fit_homography(&s, &d) {
                Some(h) if plausible(&h, profile) => h,
                _ => affine,
            }
        }
        None => affine,
    };
    let mut homography = homography;
    let mut pitch_px = homography.scale_at((0.0, 0.0))?;
    // Under perspective the affine (or even 4-point) map is off by a fraction of a pitch at
    // the halos. Re-measure the swatches and header dots around their predicted places and
    // refit; the white swatch gives the paper level for the ink weights. If the refit finds
    // too little, the coarse map stays.
    let shading = shading_for(luma, profile, &homography, pitch_px);
    let coarse = halo_stats(luma, profile, &homography, pitch_px, true, shading.as_ref())?;
    if let Ok(g) = refine_geometry(luma, profile, homography, 0.965 * coarse.white)
        && let Some(p) = g.h.scale_at((0.0, 0.0))
    {
        homography = g.h;
        pitch_px = p;
    }
    // The refined homography must now explain the halos to the strict standard; this is what
    // rejects look-alike triples.
    let shading = shading_for(luma, profile, &homography, pitch_px);
    let strict = halo_stats(
        luma,
        profile,
        &homography,
        pitch_px,
        false,
        shading.as_ref(),
    )?;
    if !strict.passes(true) {
        return None;
    }
    // A permutation test: texture can satisfy the swatch statistics anywhere, so the same
    // test on the halos displaced by a pitch (which misses the white and black swatches)
    // must fail.
    if shifted_passes(luma, profile, &homography, pitch_px, shading.as_ref()) > 1 {
        return None;
    }
    (pitch_px.is_finite() && pitch_px >= MIN_PITCH_PX).then_some(Lock {
        profile,
        homography,
        anchors_px: refined,
        center_px: center,
        pitch_px,
        score,
    })
}

fn find_center(luma: PlaneRef<'_>, h: &Homography, p: f64) -> Option<(f64, f64)> {
    let pred = h.apply(layout::center_anchor().into_pair())?;
    let blob = refine_dark_blob(luma, pred, 1.9 * p, 50.0)?;
    let area = std::f64::consts::PI
        * (dims::CENTER_RING_OUT.powi(2) - dims::CENTER_RING_IN.powi(2)
            + dims::CENTER_CORE_R.powi(2))
        * p
        * p;
    let near = (blob.at.0 - pred.0).hypot(blob.at.1 - pred.1) <= 1.0 * p;
    (near && blob.mass > 0.25 * area && blob.mass < 1.6 * area).then_some(blob.at)
}

/// Whether a 4-point homography is a sane camera view: the local pixel-per-pitch area
/// scale stays within a factor of three of the center's over the whole field.
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

/// Tiny extension so `layout::Point` converts to the `(f64, f64)` pairs used here.
trait IntoPair {
    fn into_pair(self) -> (f64, f64);
}

impl IntoPair for layout::Point {
    fn into_pair(self) -> (f64, f64) {
        (self.x, self.y)
    }
}
