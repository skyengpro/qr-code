//! From a lock to symbols: homography refinement, colour calibration, lattice sampling,
//! classification and particles.
//!
//! Stage one ([`Stage::new`]) needs only the lock: it refines the homography against the
//! anchors, the center mark, the swatches and the header dots, calibrates the four hues
//! from the swatches and classifies the 48 header dots. Stage two ([`Stage::finish`]) needs
//! the dot order, which depends on the decoded header: it fits the homography to lattice-phase
//! measurements spread over the field, classifies every dot and reads the particles.

use particlecode_core::layout::{self, DotOrder, SwatchColor, dims};
use particlecode_core::{Header, Palette, Profile, Sym3};

use crate::calib::{Calibration, HueModel, SwatchMeas};
use crate::detect::Lock;
use crate::error::VisionError;
use crate::geometry::{
    Geometry, MAX_RESIDUAL_PITCH, MIN_PITCH_PX, Pair, collect_halo_pairs, displacement,
    fit_trimmed, halo_rms, refine_geometry,
};
use crate::homography::Homography;
use crate::image::{PlaneRef, YuvRef};
use crate::lattice::{Neighbours, lattice_shift};
use crate::liveness::Liveness;
use crate::particles::read_particles;
use crate::sample::{Background, estimate_background, ink_centroid, ink_mass, mean_chroma};
use crate::size::{Feature, SizeModel, local_offsets};
use crate::util::disc_mean;

/// Radii (pitch units) of the two discs over which ink mass is summed: a core window just
/// inside a small dot and a wide window that encloses a large one. See `size`.
const MASS_RADII: [f64; 2] = [0.34, 0.52];
/// Radius (pitch units) of the disc over which chroma is averaged (`dims::SAMPLE_R`).
const CHROMA_RADIUS: f64 = dims::SAMPLE_R;

/// Everything read from one frame, before the header's error correction.
#[derive(Debug, Clone, PartialEq)]
pub struct RawRead {
    /// The decoded header, when this read went through `read_raw` (`None` for
    /// [`read_with_order`], which leaves the header to the caller).
    pub header: Option<Header>,
    /// Profile of the anchor geometry used.
    pub profile: Profile,
    /// Shape id from the header, when known.
    pub shape_id: Option<u8>,
    /// The 48 header dot symbols with confidences.
    pub header_symbols: Vec<Sym3>,
    /// One symbol per payload dot of the order that was read.
    pub body_symbols: Vec<Sym3>,
    /// Sector `0..16` of each of the six particles, if they could all be read.
    pub liveness_sectors: Option<[u8; dims::PARTICLE_COUNT]>,
    /// Refined homography: frame pitch units to image pixels. Orientation-reversing
    /// (negative determinant) when the view was mirrored.
    pub homography: Homography,
    /// Colour calibration of this frame.
    pub calibration: Calibration,
    /// RMS distance between the fitted and the observed anchors, swatches and header
    /// dots, in pitch units.
    pub residual_pitch: f64,
    /// The same residual in image pixels.
    pub residual_px: f64,
    /// Distance between the fitted small and large clusters in ln(ink mass) units; 0 when no
    /// two clusters were found and the nominal model was used. A diagnostic for blur: it
    /// shrinks as the image softens.
    pub size_separation: f64,
    /// Diagnostic: each body dot's size score, about -1 for a small dot and +1 for a large
    /// one (`NaN` if the dot was off-image).
    pub size_scores: Vec<f32>,
}

/// Read a frame completely: header, body and particles.
///
/// The header is decoded first (its profile and shape id select the dot order through
/// `order_for`); `Err(VisionError::UnknownShape)` if the closure has no order for it.
pub fn read_raw<'a>(
    frame: impl Into<YuvRef<'a>>,
    lock: &Lock,
    order_for: &dyn Fn(Profile, u8) -> Option<DotOrder>,
) -> Result<RawRead, VisionError> {
    let stage = Stage::new(frame.into(), lock)?;
    let header = Header::decode(&stage.header_symbols)?;
    if header.palette != Palette::Hues4 {
        return Err(VisionError::Header(
            particlecode_core::DecodeError::UnsupportedPalette,
        ));
    }
    if header.profile != lock.profile {
        return Err(VisionError::ProfileMismatch);
    }
    let order = order_for(header.profile, header.shape_id)
        .ok_or(VisionError::UnknownShape(header.shape_id))?;
    stage.finish(&order, Some(header))
}

/// Like [`read_raw`] for a caller that already knows the dot order (and decodes the header
/// itself): nothing is decoded, `RawRead::header` is `None`.
pub fn read_with_order<'a>(
    frame: impl Into<YuvRef<'a>>,
    lock: &Lock,
    order: &DotOrder,
) -> Result<RawRead, VisionError> {
    Stage::new(frame.into(), lock)?.finish(order, None)
}

/// The full per-frame read: header, body symbols (with confidences) and liveness.
pub fn read<'a>(
    frame: impl Into<YuvRef<'a>>,
    lock: &Lock,
    order_for: &dyn Fn(Profile, u8) -> Option<DotOrder>,
) -> Result<(Header, Vec<Sym3>, Liveness), VisionError> {
    let raw = read_raw(frame, lock, order_for)?;
    let header = raw.header.ok_or(VisionError::NoLock)?;
    let liveness = Liveness::from_sectors(raw.liveness_sectors, Some(header.packet_seed));
    Ok((header, raw.body_symbols, liveness))
}

// ---------------------------------------------------------------------------------------
// Measurement of one dot
// ---------------------------------------------------------------------------------------

/// What one dot looks like.
#[derive(Debug, Clone, Copy)]
struct DotMeas {
    /// Refined center, image pixels.
    at: (f64, f64),
    /// Mean (Cb, Cr) minus the background's.
    chroma: (f64, f64),
    /// Ink mass per pitch² inside each of [`MASS_RADII`].
    mass: [f64; 2],
    /// Mean luma over the dot's core.
    core_luma: f64,
}

fn measure(yuv: &YuvRef<'_>, h: &Homography, bg: &Background, pt: (f64, f64)) -> Option<DotMeas> {
    let pred = h.apply(pt)?;
    let area = h.area_scale(pt)?;
    let p = area.sqrt();
    let luma = yuv.luma();
    let c = match ink_centroid(luma, pred, 0.62 * p, 0.32 * p, bg.luma, 2) {
        Some(ic) if (ic.at.0 - pred.0).hypot(ic.at.1 - pred.1) <= 0.3 * p => ic.at,
        _ => pred,
    };
    let mass = [
        ink_mass(luma, c, MASS_RADII[0] * p, bg.luma)? / area,
        ink_mass(luma, c, MASS_RADII[1] * p, bg.luma)? / area,
    ];
    let (cb, cr) = mean_chroma(yuv, c, CHROMA_RADIUS * p);
    let core_luma = disc_mean(luma, c, 0.15 * p);
    Some(DotMeas {
        at: c,
        chroma: (cb - bg.cb, cr - bg.cr),
        mass,
        core_luma,
    })
}

// ---------------------------------------------------------------------------------------
// The stage state shared by header and body reading
// ---------------------------------------------------------------------------------------

/// The frame with `flat` as its luma plane, if there is one.
fn flat_view<'a>(src: &YuvRef<'a>, flat: Option<&'a [u8]>) -> YuvRef<'a> {
    let luma = src.luma();
    match flat.and_then(|f| PlaneRef::new(luma.width(), luma.height(), luma.width(), f)) {
        Some(y) => src.with_luma(y),
        None => *src,
    }
}

/// Divide a lighting gradient out of the luma plane around the code.
///
/// The paper level is fitted with a plane (see [`estimate_background`]); if it varies by more
/// than 3 % across the code, a copy of the luma plane is made in which every pixel is scaled
/// to the level at the frame center, inside the box that holds any possible shape and the
/// particle band. The returned background then has no gradient.
fn flatten(
    yuv: &YuvRef<'_>,
    h: &Homography,
    bg: Background,
    profile: Profile,
) -> (Option<Vec<u8>>, Background) {
    let luma = yuv.luma();
    let f = profile.field_half();
    let Some(p) = h.scale_at((0.0, 0.0)) else {
        return (None, bg);
    };
    if bg.variation(2.6 * f * p) < 0.03 {
        return (None, bg.flattened());
    }
    // Image box of the frame square of half-side 1.8 F (covers the disc band and the
    // corners of a square shape).
    let corners = [
        (-1.8 * f, -1.8 * f),
        (1.8 * f, -1.8 * f),
        (1.8 * f, 1.8 * f),
        (-1.8 * f, 1.8 * f),
    ];
    let mut lo = (f64::MAX, f64::MAX);
    let mut hi = (f64::MIN, f64::MIN);
    for c in corners {
        let Some(q) = h.apply(c) else {
            return (None, bg);
        };
        lo = (lo.0.min(q.0), lo.1.min(q.1));
        hi = (hi.0.max(q.0), hi.1.max(q.1));
    }
    let (w, ht) = (luma.width(), luma.height());
    let x0 = lo.0.floor().clamp(0.0, w as f64 - 1.0) as usize;
    let y0 = lo.1.floor().clamp(0.0, ht as f64 - 1.0) as usize;
    let x1 = hi.0.ceil().clamp(0.0, w as f64 - 1.0) as usize;
    let y1 = hi.1.ceil().clamp(0.0, ht as f64 - 1.0) as usize;
    let mut out = Vec::with_capacity(w * ht);
    for y in 0..ht {
        out.extend_from_slice(luma.row(y));
    }
    for y in y0..=y1 {
        for x in x0..=x1 {
            let level = bg.at(x as f64, y as f64).max(8.0);
            let gain = (bg.luma / level).clamp(0.5, 2.0);
            let v = f64::from(out[y * w + x]) * gain;
            out[y * w + x] = (v + 0.5).clamp(0.0, 255.0) as u8;
        }
    }
    (Some(out), bg.flattened())
}

/// Reflection of the frame about its vertical axis.
const MIRROR_X: Homography = Homography([[-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);

/// Refine the geometry from `start` and calibrate the colours through it.
fn align(
    yuv: &YuvRef<'_>,
    lock: &Lock,
    start: Homography,
    bg: &Background,
) -> Result<(Geometry, Calibration, HueModel), VisionError> {
    let geo = refine_geometry(yuv.luma(), lock.profile, start, bg.luma)?;
    let (cal, model) = calibrate(yuv, &geo.h, bg, lock.profile)?;
    if !model.wheel_order_ok() {
        return Err(VisionError::Lighting);
    }
    Ok((geo, cal, model))
}

struct Stage<'a> {
    /// The frame as captured.
    src: YuvRef<'a>,
    /// Luma with the lighting gradient divided out, when there was one worth removing.
    flat: Option<Vec<u8>>,
    profile: Profile,
    h: Homography,
    bg: Background,
    pairs: Vec<Pair>,
    residual_pitch: f64,
    residual_px: f64,
    cal: Calibration,
    model: HueModel,
    header_symbols: Vec<Sym3>,
}

impl<'a> Stage<'a> {
    fn new(yuv: YuvRef<'a>, lock: &Lock) -> Result<Stage<'a>, VisionError> {
        let luma = yuv.luma();
        if luma.width() < 48 || luma.height() < 48 {
            return Err(VisionError::NoLock);
        }
        let profile = lock.profile;
        let p0 = lock
            .homography
            .scale_at((0.0, 0.0))
            .ok_or(VisionError::NoLock)?;
        if !(p0.is_finite() && p0 >= MIN_PITCH_PX) {
            return Err(VisionError::NoLock);
        }
        let f = profile.field_half();
        let bg = estimate_background(&yuv, &lock.homography, (1.07 * f, 1.18 * f), 0.85)
            .ok_or(VisionError::Truncated)?;
        let (flat, bg) = flatten(&yuv, &lock.homography, bg, profile);
        let view = flat_view(&yuv, flat.as_deref());
        let (geo, cal, model) = match align(&view, lock, lock.homography, &bg) {
            Ok(found) => found,
            // The halo is mirror-symmetric, so `locate` cannot tell a mirrored (selfie) view
            // from a normal one; only the hue order around the swatch rings differs. If the
            // colours do not calibrate, try the mirrored frame before giving up.
            Err(VisionError::Lighting) => {
                let mirrored = lock.homography.then_after(&MIRROR_X);
                align(&view, lock, mirrored, &bg).map_err(|_| VisionError::Lighting)?
            }
            Err(e) => return Err(e),
        };
        let header_symbols = read_header(&view, &geo.h, &bg, &cal, &model, profile);
        Ok(Stage {
            src: yuv,
            flat,
            profile,
            h: geo.h,
            bg,
            pairs: geo.pairs,
            residual_pitch: geo.residual_pitch,
            residual_px: geo.residual_px,
            cal,
            model,
            header_symbols,
        })
    }

    /// The frame as the measurements should see it (illumination flattened).
    fn yuv(&self) -> YuvRef<'_> {
        flat_view(&self.src, self.flat.as_deref())
    }

    fn finish(mut self, order: &DotOrder, header: Option<Header>) -> Result<RawRead, VisionError> {
        if order.profile() != self.profile {
            return Err(VisionError::ProfileMismatch);
        }
        let band = order.particle_band();
        // The true particle band is empty paper: a better background than the guess.
        if let Some(b) = estimate_background(
            &self.src,
            &self.h,
            (band.inner + 0.8, band.outer - 0.8),
            0.5,
        ) {
            let (flat, bg) = flatten(&self.src, &self.h, b, self.profile);
            if (bg.luma - self.bg.luma).abs() > 1.0
                && (bg.luma - self.bg.luma).abs() < 0.1 * self.bg.luma
                || flat.is_some() != self.flat.is_some()
            {
                self.flat = flat;
                self.bg = bg;
                let (cal, model) = calibrate(&self.yuv(), &self.h, &self.bg, self.profile)?;
                self.cal = cal;
                self.model = model;
            }
        }
        self.refit_with_patches(order)?;
        let body = self.read_body(order);
        let liveness_sectors = read_particles(self.yuv().luma(), &self.h, band, self.bg.luma);
        Ok(RawRead {
            shape_id: header.map(|h| h.shape_id),
            header,
            profile: self.profile,
            header_symbols: self.header_symbols,
            body_symbols: body.symbols,
            liveness_sectors,
            homography: self.h,
            calibration: self.cal,
            residual_pitch: self.residual_pitch,
            residual_px: self.residual_px,
            size_separation: body.separation,
            size_scores: body.scores,
        })
    }

    /// Fit the homography to lattice-phase measurements spread over the whole field.
    ///
    /// The halo-only homography is accurate near the anchors and drifts quadratically away
    /// from them, and `lattice_shift` only resolves displacements below about 0.4 pitch, so
    /// the set of patches grows outwards: each pass trusts patches close enough to the
    /// anchors for the previous fit to be good to well under that.
    fn refit_with_patches(&mut self, order: &DotOrder) -> Result<(), VisionError> {
        let profile = self.profile;
        let anchors = layout::hero_anchors(profile);
        let near = |c: (f64, f64)| {
            anchors
                .iter()
                .map(|a| (a.x - c.0).hypot(a.y - c.1))
                .fold(f64::MAX, f64::min)
        };
        // Windows must keep clear of the halos and the center mark.
        let mut centres: Vec<(f64, f64)> = order
            .points()
            .iter()
            .map(|p| (p.at.x, p.at.y))
            .filter(|&c| near(c) >= 7.5 && c.0.hypot(c.1) >= 4.5)
            .collect();
        let stride = (centres.len() / 160).max(1);
        centres = centres.into_iter().step_by(stride).collect();
        if centres.len() < 12 {
            return Ok(());
        }
        let mut reaches = vec![12.0, 16.0, 22.0, 30.0, 42.0];
        reaches.extend([f64::MAX; 4]);
        for reach in reaches {
            let mut pairs = self.pairs.clone();
            let base = pairs.len();
            for &c in centres.iter().filter(|&&c| near(c) <= reach) {
                if let Some(d) = lattice_shift(self.yuv().luma(), &self.h, self.bg.luma, c)
                    && let Some(dst) = self.h.apply((c.0 + d.0, c.1 + d.1))
                {
                    pairs.push(Pair {
                        src: c,
                        dst,
                        weight: 3,
                    });
                }
            }
            if pairs.len() < base + 6 {
                continue;
            }
            let Some((h, _, kept)) = fit_trimmed(&pairs, profile) else {
                continue;
            };
            if kept.len() * 2 <= pairs.len() {
                continue;
            }
            // Keep the old geometry if the new one explains the halos clearly worse.
            let old = halo_rms(&self.h, &self.pairs);
            let new = halo_rms(&h, &self.pairs);
            if new > old * 1.5 + 0.03 {
                continue;
            }
            let moved = displacement(&self.h, &h, profile);
            self.h = h;
            if reach > 1e6 && moved < 0.01 {
                break;
            }
        }
        // The halo pairs were measured from the coarse homography and are biased towards
        // it; measure them again from the refined one and report that residual.
        let (halo, _) = collect_halo_pairs(self.yuv().luma(), &self.h, self.bg.luma, profile)?;
        if let Some((_, rms, kept)) = fit_trimmed(&halo, profile)
            && kept.len() * 10 >= halo.len() * 6
        {
            self.residual_pitch = halo_rms(&self.h, &halo);
            let _ = rms;
            self.residual_px = self.residual_pitch * self.h.scale_at((0.0, 0.0)).unwrap_or(1.0);
            self.pairs = halo;
        }
        if self.residual_pitch > MAX_RESIDUAL_PITCH {
            return Err(VisionError::Residual);
        }
        Ok(())
    }

    fn read_body(&self, order: &DotOrder) -> BodyRead {
        let pts = order.points();
        let yuv = self.yuv();
        let dots: Vec<Option<ClassifiedDot>> = pts
            .iter()
            .map(|p| {
                measure(&yuv, &self.h, &self.bg, (p.at.x, p.at.y))
                    .map(|m| classify_dot(&m, &self.model, &self.cal, &self.bg))
            })
            .collect();
        let features: Vec<Feature> = dots.iter().flatten().map(|d| d.feature).collect();
        let model = SizeModel::fit(&features);
        let scores: Vec<f64> = dots
            .iter()
            .map(|d| d.as_ref().map_or(f64::NAN, |d| model.score(&d.feature)))
            .collect();
        let offsets = local_offsets(pts, &scores);
        let neighbours = Neighbours::new(pts);
        let symbols = dots
            .iter()
            .enumerate()
            .zip(scores.iter().zip(&offsets))
            .map(|((i, d), (&score, &offset))| match d {
                Some(d) => {
                    let mut around: Vec<f64> = neighbours
                        .of(&pts[i])
                        .iter()
                        .flatten()
                        .map(|&j| scores[j as usize])
                        .filter(|s| s.is_finite())
                        .collect();
                    let median = (around.len() >= 3).then(|| crate::util::median(&mut around));
                    let (bit, size_conf) = model.decide_with_neighbours(score, offset, median);
                    Sym3::from_parts(d.color, bit).with_confidence(confidence(&[
                        d.hue_conf,
                        size_conf,
                        d.depth_conf,
                    ]))
                }
                None => Sym3 {
                    value: 0,
                    confidence: 0,
                },
            })
            .collect();
        BodyRead {
            symbols,
            separation: if model.fitted { model.separation } else { 0.0 },
            scores: scores.iter().map(|&s| s as f32).collect(),
        }
    }
}

/// A dot after colour classification, before the size decision.
#[derive(Debug, Clone, Copy)]
struct ClassifiedDot {
    color: u8,
    /// Confidence of the hue decision, `0..=1`.
    hue_conf: f64,
    /// Natural logs of the ink masses relative to the swatches of the same hue.
    feature: Feature,
    /// Confidence that the dot is not washed out (glare, shadow), `0..=1`.
    depth_conf: f64,
}

fn classify_dot(
    m: &DotMeas,
    model: &HueModel,
    cal: &Calibration,
    bg: &Background,
) -> ClassifiedDot {
    let (color, hue_conf) = model.classify(m.chroma);
    let k = usize::from(color);
    // A dot that has lost depth (its core is much lighter than a swatch core of the same hue)
    // is washed out by glare or a highlight, whatever its measured size says.
    let full = (bg.luma - cal.hue_luma[k]).max(1.0);
    let lost = (m.core_luma - cal.hue_luma[k]) / full;
    let depth_conf = ((0.55 - lost) / 0.3).clamp(0.0, 1.0);
    let feature = std::array::from_fn(|w| (m.mass[w] / cal.swatch_mass[w][k]).max(1e-3).ln());
    ClassifiedDot {
        color,
        hue_conf,
        feature,
        depth_conf,
    }
}

/// Symbol confidence `0..=255` from independent evidence factors in `0..=1`.
fn confidence(factors: &[f64]) -> u8 {
    let worst = factors.iter().copied().fold(1.0, f64::min);
    (worst * 255.0).round().clamp(0.0, 255.0) as u8
}

/// Output of [`Stage::read_body`].
struct BodyRead {
    symbols: Vec<Sym3>,
    separation: f64,
    scores: Vec<f32>,
}

/// Measure the swatches and build the calibration.
fn calibrate(
    yuv: &YuvRef<'_>,
    h: &Homography,
    bg: &Background,
    profile: Profile,
) -> Result<(Calibration, HueModel), VisionError> {
    let luma = yuv.luma();
    let mut meas: Vec<SwatchMeas> = Vec::with_capacity(48);
    let (mut white, mut black) = (None, None);
    for a in 0..3 {
        for s in 0..dims::HALO_SLOTS {
            let sw = layout::swatch(profile, a, s);
            let pt = (sw.at.x, sw.at.y);
            match sw.color {
                SwatchColor::White => {
                    if let (Some(q), Some(p)) = (h.apply(pt), h.scale_at(pt))
                        && luma.contains(q.0, q.1, 2.0)
                    {
                        white = Some(disc_mean(luma, q, 0.2 * p));
                    }
                }
                SwatchColor::Black => {
                    if let (Some(q), Some(p)) = (h.apply(pt), h.scale_at(pt))
                        && luma.contains(q.0, q.1, 2.0)
                    {
                        black = Some(disc_mean(luma, q, 0.15 * p));
                    }
                }
                SwatchColor::Hue(k) => {
                    if let Some(m) = measure(yuv, h, bg, pt) {
                        let l = f64::from(luma.bilinear(m.at.0, m.at.1));
                        meas.push(SwatchMeas {
                            hue: usize::from(k),
                            chroma: m.chroma,
                            mass: m.mass,
                            luma: l,
                        });
                    }
                }
            }
        }
    }
    Calibration::from_swatches(&meas, bg.luma, (bg.cb, bg.cr), white, black)
}

fn read_header(
    yuv: &YuvRef<'_>,
    h: &Homography,
    bg: &Background,
    cal: &Calibration,
    model: &HueModel,
    profile: Profile,
) -> Vec<Sym3> {
    let dots: Vec<Option<ClassifiedDot>> = (0..layout::HEADER_DOTS)
        .map(|j| {
            let p = layout::header_dot(profile, j);
            measure(yuv, h, bg, (p.x, p.y)).map(|m| classify_dot(&m, model, cal, bg))
        })
        .collect();
    let features: Vec<Feature> = dots.iter().flatten().map(|d| d.feature).collect();
    let size = SizeModel::fit(&features);
    dots.iter()
        .map(|d| match d {
            Some(d) => {
                let (bit, size_conf) = size.decide(size.score(&d.feature), 0.0);
                Sym3::from_parts(d.color, bit).with_confidence(confidence(&[
                    d.hue_conf,
                    size_conf,
                    d.depth_conf,
                ]))
            }
            None => Sym3 {
                value: 0,
                confidence: 0,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_is_the_weakest_factor() {
        assert_eq!(confidence(&[1.0, 1.0, 1.0]), 255);
        assert_eq!(confidence(&[1.0, 0.5, 0.9]), 128);
        assert_eq!(confidence(&[0.0, 1.0]), 0);
    }
}
