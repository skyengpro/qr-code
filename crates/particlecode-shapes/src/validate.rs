//! Rule 7: synthetic validation against the disc.
//!
//! The spec asks for 1,000 synthetic frames of the shape, degraded the way a camera would
//! (perspective, blur, glare, exposure, colour cast, noise), decoded, and the symbol error
//! rate (SER) compared with the disc's under the same degradations. A shape passes at
//! `<= 1.3x` the disc's rate.
//!
//! Done here: the synthetic half. [`SyntheticFrames`] yields reproducible degraded frames
//! of any [`DotOrder`] together with the ground-truth symbols, and [`judge`] turns two
//! error counts into the verdict.
//!
//! Missing, pending the vision decoder: decoding each [`SyntheticFrame::image`], aligning
//! the decoded symbols with [`SyntheticFrame::spec`]`.all_symbols()`, and counting errors
//! into [`SerCount`]. That is the one `TODO` in [`compare_with_disc`].

use particlecode_core::frame::FrameSpec;
use particlecode_core::layout::DotOrder;
use particlecode_core::profile::{Profile, Rgb};
use particlecode_core::shapes::{DISC, ShapeTable};
use particlecode_render::synth::{CameraPose, Degradation, RgbImage, Rng, apply_all};
use particlecode_render::{RenderConfig, render};

use crate::error::Error;
use crate::preview::random_frame;

/// Rule 7: a shape passes at this multiple of the disc's symbol error rate.
pub const PASS_RATIO: f64 = 1.3;
/// Frames the spec asks for.
pub const DEFAULT_FRAMES: usize = 1000;

/// Colour behind the warped screen (a dark desk).
const SURROUND: Rgb = Rgb(70, 72, 75);

/// What to generate.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidateConfig {
    /// Lattice density.
    pub profile: Profile,
    /// Number of frames.
    pub frames: usize,
    /// Seed of symbols and degradations. The shape and the disc use the same seed, so both
    /// see identical poses, blur, glare and noise.
    pub seed: u64,
    /// Side of the clean render in pixels.
    pub render_px: u32,
    /// Size of the degraded camera image `(width, height)`.
    pub camera_px: (u32, u32),
}

impl Default for ValidateConfig {
    fn default() -> Self {
        ValidateConfig {
            profile: Profile::M,
            frames: DEFAULT_FRAMES,
            seed: 7,
            render_px: 1000,
            camera_px: (960, 720),
        }
    }
}

/// One degraded frame and what it truly contains.
#[derive(Debug, Clone)]
pub struct SyntheticFrame {
    /// Index in the run.
    pub index: usize,
    /// The frame that was rendered; `spec.all_symbols()` is the ground truth (48 header
    /// symbols, then the body in dot order).
    pub spec: FrameSpec,
    /// The degradations applied to the clean render, in order.
    pub chain: Vec<Degradation>,
    /// The degraded camera image.
    pub image: RgbImage,
}

/// Iterator over the synthetic frames of one shape.
#[derive(Debug)]
pub struct SyntheticFrames<'a> {
    order: &'a DotOrder,
    shape_id: u8,
    table_version: u8,
    cfg: ValidateConfig,
    next: usize,
}

impl<'a> SyntheticFrames<'a> {
    /// Frames of `order` (shape `shape_id` of a table at `table_version`).
    pub fn new(
        order: &'a DotOrder,
        shape_id: u8,
        table_version: u8,
        cfg: ValidateConfig,
    ) -> SyntheticFrames<'a> {
        SyntheticFrames {
            order,
            shape_id,
            table_version,
            cfg,
            next: 0,
        }
    }

    /// Frame `index` of the run. Equal `(order, config, index)` give equal frames.
    pub fn frame(&self, index: usize) -> Option<SyntheticFrame> {
        let render_cfg = RenderConfig {
            size_px: self.cfg.render_px,
            ..RenderConfig::default()
        };
        render_cfg.validate().ok()?;
        // Symbols and degradations come from separate streams so the shape and the disc
        // (which have different dot counts) still see the same camera.
        let frame_seed = self.cfg.seed ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut symbols = Rng::new(frame_seed);
        let mut camera = Rng::new(frame_seed ^ 0xCA3E_7A00);
        let spec = random_frame(
            self.order,
            self.shape_id,
            self.table_version,
            index as u32,
            &mut symbols,
        );
        let clean = RgbImage::from_pixmap(&render(&spec, &render_cfg));
        let chain = random_chain(&mut camera, self.cfg.camera_px);
        let image = apply_all(&clean, &chain);
        Some(SyntheticFrame {
            index,
            spec,
            chain,
            image,
        })
    }
}

impl Iterator for SyntheticFrames<'_> {
    type Item = SyntheticFrame;

    fn next(&mut self) -> Option<SyntheticFrame> {
        if self.next >= self.cfg.frames {
            return None;
        }
        let f = self.frame(self.next);
        self.next += 1;
        f
    }
}

/// A random camera: tilt and rotate, blur, glare, exposure, tint, sensor noise, in that order.
pub fn random_chain(rng: &mut Rng, (w, h): (u32, u32)) -> Vec<Degradation> {
    let range = |rng: &mut Rng, lo: f64, hi: f64| lo + (hi - lo) * rng.next_f64();
    let mut chain = vec![Degradation::Tilt {
        pose: CameraPose {
            rotate_deg: range(rng, -20.0, 20.0),
            tilt_x_deg: range(rng, -25.0, 25.0),
            tilt_y_deg: range(rng, -25.0, 25.0),
            scale: range(rng, 0.72, 0.86),
        },
        out_w: w,
        out_h: h,
        background: SURROUND,
    }];
    chain.push(Degradation::Blur {
        sigma: range(rng, 0.4, 1.4),
    });
    if rng.next_f64() < 0.5 {
        chain.push(Degradation::Glare {
            center: (
                range(rng, 0.2, 0.8) * f64::from(w),
                range(rng, 0.2, 0.8) * f64::from(h),
            ),
            radius: range(rng, 0.08, 0.2) * f64::from(w),
            strength: range(rng, 0.15, 0.5),
        });
    }
    chain.push(Degradation::Exposure {
        gain: range(rng, 0.85, 1.15),
        offset: range(rng, -12.0, 12.0),
    });
    chain.push(Degradation::ColorCast {
        r: range(rng, 0.93, 1.07),
        g: range(rng, 0.93, 1.07),
        b: range(rng, 0.93, 1.07),
    });
    chain.push(Degradation::Noise {
        sigma: range(rng, 2.0, 8.0),
        seed: rng.next_u64(),
    });
    chain
}

/// Symbol errors of one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SerCount {
    /// Symbols compared (frames that could not be located count all their symbols as
    /// wrong).
    pub symbols: u64,
    /// Symbols that differ from the ground truth.
    pub errors: u64,
}

impl SerCount {
    /// Symbol error rate, `0` if nothing was compared.
    pub fn rate(&self) -> f64 {
        if self.symbols == 0 {
            0.0
        } else {
            self.errors as f64 / self.symbols as f64
        }
    }
}

/// Rule 7 verdict.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Verdict {
    /// The shape's symbol error rate.
    pub shape_ser: f64,
    /// The disc's symbol error rate under the same degradations.
    pub disc_ser: f64,
    /// `shape_ser / disc_ser`.
    pub ratio: f64,
    /// `ratio <= PASS_RATIO`.
    pub passed: bool,
}

/// Compare a shape's error count with the disc's. A disc without errors only lets a shape
/// without errors pass.
pub fn judge(shape: SerCount, disc: SerCount) -> Verdict {
    let (shape_ser, disc_ser) = (shape.rate(), disc.rate());
    let ratio = if disc_ser > 0.0 {
        shape_ser / disc_ser
    } else if shape_ser > 0.0 {
        f64::INFINITY
    } else {
        1.0
    };
    Verdict {
        shape_ser,
        disc_ser,
        ratio,
        passed: ratio <= PASS_RATIO,
    }
}

/// Run rule 7 for `shape` against the disc: render `cfg.frames` degraded frames of each,
/// decode them and compare symbol error rates.
///
/// Currently returns [`Error::DecoderUnavailable`] after the generators are set up.
pub fn compare_with_disc(
    shape: &DotOrder,
    shape_id: u8,
    table: &ShapeTable,
    cfg: &ValidateConfig,
) -> Result<Verdict, Error> {
    let disc = table
        .dot_order(DISC, cfg.profile)
        .ok_or_else(|| Error::BadOption("table has no disc".into()))?;
    let _shape_frames = SyntheticFrames::new(shape, shape_id, table.version(), cfg.clone());
    let _disc_frames = SyntheticFrames::new(&disc, DISC, table.version(), cfg.clone());
    // TODO(vision): for each frame, call the vision decoder on `frame.image`, align the
    // recovered header+body symbols with `frame.spec.all_symbols()`, and add the number of
    // wrong symbols (all of them if the frame was not located) to a `SerCount` per shape;
    // then `Ok(judge(shape_count, disc_count))`.
    Err(Error::DecoderUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_cfg(frames: usize) -> ValidateConfig {
        ValidateConfig {
            profile: Profile::S,
            frames,
            seed: 3,
            render_px: 300,
            camera_px: (320, 240),
        }
    }

    #[test]
    fn frames_are_deterministic_and_degraded() {
        let table = ShapeTable::builtin();
        let order = table.dot_order(30, Profile::S).unwrap();
        let gen_a = SyntheticFrames::new(&order, 30, 0, small_cfg(2));
        let a0 = gen_a.frame(0).unwrap();
        let a0_again = gen_a.frame(0).unwrap();
        let a1 = gen_a.frame(1).unwrap();
        assert_eq!(a0.image, a0_again.image);
        assert_ne!(a0.image, a1.image);
        assert_eq!((a0.image.width, a0.image.height), (320, 240));
        assert_eq!(a0.spec.dots.len(), order.len());
        assert_eq!(a0.spec.all_symbols().len(), 48 + order.len());
        assert!(matches!(a0.chain[0], Degradation::Tilt { .. }));
        assert_eq!(gen_a.count(), 2);
    }

    #[test]
    fn shape_and_disc_share_the_camera() {
        let table = ShapeTable::builtin();
        let ring = table.dot_order(30, Profile::S).unwrap();
        let disc = table.dot_order(0, Profile::S).unwrap();
        let a = SyntheticFrames::new(&ring, 30, 0, small_cfg(1))
            .frame(0)
            .unwrap();
        let b = SyntheticFrames::new(&disc, 0, 0, small_cfg(1))
            .frame(0)
            .unwrap();
        assert_eq!(a.chain, b.chain);
    }

    #[test]
    fn verdict_threshold() {
        let disc = SerCount {
            symbols: 10_000,
            errors: 100,
        };
        let ok = SerCount {
            symbols: 5_000,
            errors: 65,
        };
        let bad = SerCount {
            symbols: 5_000,
            errors: 66,
        };
        assert!(judge(ok, disc).passed);
        assert!(!judge(bad, disc).passed);
        let clean = SerCount {
            symbols: 10,
            errors: 0,
        };
        assert!(judge(clean, clean).passed);
        assert!(!judge(ok, clean).passed);
    }

    #[test]
    fn comparison_waits_for_the_decoder() {
        let table = ShapeTable::builtin();
        let order = table.dot_order(30, Profile::S).unwrap();
        let err = compare_with_disc(&order, 30, &table, &small_cfg(1)).unwrap_err();
        assert_eq!(err, Error::DecoderUnavailable);
    }
}
