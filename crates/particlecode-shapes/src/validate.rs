//! Rule 7: synthetic validation against the disc.
//!
//! The spec asks for 1,000 synthetic frames of the shape, degraded the way a camera would
//! (perspective, blur, glare, exposure, colour cast, noise), decoded, and the symbol error
//! rate (SER) compared with the disc's under the same degradations. A shape passes at
//! `<= 1.3x` the disc's rate.
//!
//! The pipeline per frame:
//!
//! 1. [`SyntheticFrames`] builds a *real* frame: random data bytes, whitened and
//!    Reed-Solomon protected by [`BodyCodec`], a valid encoded [`Header`], random particle
//!    sectors; renders it with `particlecode-render`; and degrades the picture with a
//!    random camera chain ([`random_chain`]). The shape and the disc see identical camera
//!    chains frame by frame.
//! 2. [`decode_frame`] converts the image to 4:2:0 YUV, runs
//!    [`particlecode_vision::locate`] and [`particlecode_vision::read_with_order`] with the
//!    known dot order, and compares the 48 header and all body symbols with the truth. A
//!    frame that does not lock or read counts every symbol as wrong.
//! 3. [`run_shape`] spreads the frames over threads and sums a [`RunStats`];
//!    [`compare_with_disc`] runs the shape and the disc and returns the [`Verdict`].

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use particlecode_core::body::{BodyCodec, WhitenKey};
use particlecode_core::frame::FrameSpec;
use particlecode_core::header::Header;
use particlecode_core::layout::{DotOrder, dims};
use particlecode_core::profile::{EccLevel, Profile, Rgb};
use particlecode_core::rs::ERASURE_BELOW;
use particlecode_core::shapes::{DISC, ShapeTable};
use particlecode_render::demo::demo_header;
use particlecode_render::synth::{CameraPose, Degradation, RgbImage, Rng, apply_all};
use particlecode_render::{RenderConfig, render};
use particlecode_vision::{YuvFrame, locate, read_with_order};

use crate::error::Error;

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
    /// Worker threads; 0 means one per available core.
    pub threads: usize,
}

impl Default for ValidateConfig {
    fn default() -> Self {
        ValidateConfig {
            profile: Profile::M,
            frames: DEFAULT_FRAMES,
            seed: 7,
            render_px: 1000,
            camera_px: (960, 720),
            threads: 0,
        }
    }
}

impl ValidateConfig {
    fn check(&self) -> Result<(), Error> {
        RenderConfig {
            size_px: self.render_px,
            ..RenderConfig::default()
        }
        .validate()
        .map_err(|e| Error::BadOption(e.to_string()))?;
        if self.camera_px.0 < 64 || self.camera_px.1 < 64 {
            return Err(Error::BadOption(
                "camera image is smaller than 64 px".into(),
            ));
        }
        Ok(())
    }

    fn thread_count(&self) -> usize {
        let n = if self.threads == 0 {
            thread::available_parallelism().map_or(1, usize::from)
        } else {
            self.threads
        };
        n.clamp(1, self.frames.max(1))
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
    /// The data bytes the body carries (`BodyCodec::block_size` of them).
    pub data: Vec<u8>,
    /// Whitening key of the body.
    pub key: WhitenKey,
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

        let mut header = demo_header(self.order.profile(), index as u32);
        header.shape_id = self.shape_id;
        header.shape_table_version = self.table_version & 3;
        // The demo header advances the segment index every 8 frames, which a 3 KB payload
        // does not have; keep one segment so the header always decodes.
        header.segment_index = 0;
        let (spec, data, key) = encoded_frame(self.order, header, index as u32, &mut symbols);
        let clean = RgbImage::from_pixmap(&render(&spec, &render_cfg));
        let chain = random_chain(&mut camera, self.cfg.camera_px);
        let image = apply_all(&clean, &chain);
        Some(SyntheticFrame {
            index,
            spec,
            data,
            key,
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

/// A frame carrying random data through the real body and header codecs.
fn encoded_frame(
    order: &DotOrder,
    header: Header,
    index: u32,
    rng: &mut Rng,
) -> (FrameSpec, Vec<u8>, WhitenKey) {
    let codec = BodyCodec::new(order.len(), EccLevel::Normal);
    let data: Vec<u8> = (0..codec.block_size())
        .map(|_| rng.below(256) as u8)
        .collect();
    let key = WhitenKey {
        session_id: header.session_id,
        segment_index: header.segment_index,
        packet_seed: header.packet_seed,
    };
    let body = codec.encode(&data, key);
    let mut sectors = [0u8; dims::PARTICLE_COUNT];
    for s in &mut sectors {
        *s = rng.below(dims::PARTICLE_SECTORS as u64) as u8;
    }
    let spec = FrameSpec::assemble(index, header, &header.encode(), order, &body, sectors);
    (spec, data, key)
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

/// What decoding one frame gave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)] // reason: independent pipeline stages, each a yes/no
pub struct FrameOutcome {
    /// `locate` found the anchors.
    pub located: bool,
    /// `read_with_order` produced symbols.
    pub read: bool,
    /// Symbols in the frame (header + body).
    pub symbols: u64,
    /// Symbols that differ from the truth (all of them if the frame was not read).
    pub errors: u64,
    /// Symbols the reader returned (0 if the frame was not read).
    pub read_symbols: u64,
    /// Of those, symbols below [`ERASURE_BELOW`] confidence.
    pub erasures: u64,
    /// The 48 header dots decode to the true header.
    pub header_decoded: bool,
    /// The Reed-Solomon body decode returns the original data bytes.
    pub body_decoded: bool,
}

/// Decode `frame` with the vision crate and compare with the truth.
pub fn decode_frame(frame: &SyntheticFrame, order: &DotOrder) -> FrameOutcome {
    let truth_header = frame.spec.header_symbols();
    let truth_body = frame.spec.body_symbols();
    let mut out = FrameOutcome {
        symbols: (truth_header.len() + truth_body.len()) as u64,
        ..FrameOutcome::default()
    };
    out.errors = out.symbols;

    let img = &frame.image;
    let yuv =
        YuvFrame::from_rgb(img.width as usize, img.height as usize, &img.data).subsample_chroma();
    let Some(lock) = locate(&yuv.y) else {
        return out;
    };
    out.located = true;
    let Ok(raw) = read_with_order(&yuv, &lock, order) else {
        return out;
    };
    if raw.header_symbols.len() != truth_header.len() || raw.body_symbols.len() != truth_body.len()
    {
        return out;
    }
    out.read = true;
    let wrong = |read: &[particlecode_core::Sym3], truth: &[particlecode_core::Sym3]| {
        read.iter()
            .zip(truth)
            .filter(|(a, b)| a.value != b.value)
            .count() as u64
    };
    out.errors = wrong(&raw.header_symbols, &truth_header) + wrong(&raw.body_symbols, &truth_body);
    out.read_symbols = (raw.header_symbols.len() + raw.body_symbols.len()) as u64;
    out.erasures = raw
        .header_symbols
        .iter()
        .chain(&raw.body_symbols)
        .filter(|s| s.confidence < ERASURE_BELOW)
        .count() as u64;
    out.header_decoded = Header::decode(&raw.header_symbols).is_ok_and(|h| h == frame.spec.header);
    let codec = BodyCodec::new(order.len(), EccLevel::Normal);
    out.body_decoded = codec
        .decode(&raw.body_symbols, frame.key)
        .is_ok_and(|d| d == frame.data);
    out
}

/// Sums of [`FrameOutcome`]s over a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunStats {
    /// Frames decoded.
    pub frames: u64,
    /// Frames the anchors were found in.
    pub located: u64,
    /// Frames that produced symbols.
    pub read: u64,
    /// Symbols compared.
    pub symbols: u64,
    /// Symbols wrong (unread frames count all their symbols).
    pub errors: u64,
    /// Symbols wrong in the frames that were read.
    pub read_errors: u64,
    /// Symbols returned by the reader.
    pub read_symbols: u64,
    /// Returned symbols below the erasure confidence.
    pub erasures: u64,
    /// Frames whose header decoded.
    pub header_decoded: u64,
    /// Frames whose body decoded to the original data.
    pub body_decoded: u64,
    /// Frames where both did.
    pub fully_decoded: u64,
}

impl RunStats {
    /// Add one frame.
    pub fn add(&mut self, o: &FrameOutcome) {
        self.frames += 1;
        self.located += u64::from(o.located);
        self.read += u64::from(o.read);
        self.symbols += o.symbols;
        self.errors += o.errors;
        if o.read {
            self.read_errors += o.errors;
        }
        self.read_symbols += o.read_symbols;
        self.erasures += o.erasures;
        self.header_decoded += u64::from(o.header_decoded);
        self.body_decoded += u64::from(o.body_decoded);
        self.fully_decoded += u64::from(o.header_decoded && o.body_decoded);
    }

    /// Add another run's sums.
    pub fn merge(&mut self, other: &RunStats) {
        self.frames += other.frames;
        self.located += other.located;
        self.read += other.read;
        self.symbols += other.symbols;
        self.errors += other.errors;
        self.read_errors += other.read_errors;
        self.read_symbols += other.read_symbols;
        self.erasures += other.erasures;
        self.header_decoded += other.header_decoded;
        self.body_decoded += other.body_decoded;
        self.fully_decoded += other.fully_decoded;
    }

    /// Symbol error rate (unread frames count as all wrong).
    pub fn ser(&self) -> f64 {
        ratio(self.errors, self.symbols)
    }

    /// Fraction of frames that locked.
    pub fn lock_rate(&self) -> f64 {
        ratio(self.located, self.frames)
    }

    /// Fraction of frames that were read.
    pub fn read_rate(&self) -> f64 {
        ratio(self.read, self.frames)
    }

    /// Fraction of read symbols that would be Reed-Solomon erasures.
    pub fn erasure_rate(&self) -> f64 {
        ratio(self.erasures, self.read_symbols)
    }

    /// Fraction of frames that fully decode.
    pub fn decode_rate(&self) -> f64 {
        ratio(self.fully_decoded, self.frames)
    }

    /// Errors and symbols as a [`SerCount`].
    pub fn count(&self) -> SerCount {
        SerCount {
            symbols: self.symbols,
            errors: self.errors,
        }
    }

    /// Errors and symbols of the frames that were read.
    pub fn read_count(&self) -> SerCount {
        SerCount {
            symbols: self.read_symbols,
            errors: self.read_errors,
        }
    }

    /// Symbol error rate over the frames that were read.
    pub fn read_ser(&self) -> f64 {
        self.read_count().rate()
    }
}

fn ratio(a: u64, b: u64) -> f64 {
    if b == 0 { 0.0 } else { a as f64 / b as f64 }
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
        ratio(self.errors, self.symbols)
    }
}

/// Rule 7 verdict.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Verdict {
    /// The shape's symbol error rate.
    pub shape_ser: f64,
    /// The disc's symbol error rate under the same degradations.
    pub disc_ser: f64,
    /// `shape_ser / disc_ser` (with the disc's error count floored at one, see [`judge`]).
    pub ratio: f64,
    /// The same ratio over the frames that were read, i.e. with lock failures left out
    /// (they happen to shape and disc alike and, counted as all-wrong, dominate the rate).
    /// `1.0` if no frame was read.
    pub read_ratio: f64,
    /// `ratio <= PASS_RATIO && read_ratio <= PASS_RATIO`.
    pub passed: bool,
}

/// Compare a shape's error count with the disc's. If the disc made no error at all the
/// ratio would be undefined, so the disc's count is taken as one error: the shape then
/// passes with at most 1.3 errors per disc-sized sample, i.e. with none.
pub fn judge(shape: SerCount, disc: SerCount) -> Verdict {
    let ratio = error_ratio(shape, disc);
    Verdict {
        shape_ser: shape.rate(),
        disc_ser: disc.rate(),
        ratio,
        read_ratio: 1.0,
        passed: ratio <= PASS_RATIO + 1e-9,
    }
}

/// The verdict of two runs: the spec's ratio over all symbols (frames that do not lock
/// count as entirely wrong) and the same ratio over the frames that were read; both must be
/// within [`PASS_RATIO`].
pub fn judge_runs(shape: &RunStats, disc: &RunStats) -> Verdict {
    let all = judge(shape.count(), disc.count());
    let read_ratio = error_ratio(shape.read_count(), disc.read_count());
    Verdict {
        read_ratio,
        passed: all.passed && read_ratio <= PASS_RATIO + 1e-9,
        ..all
    }
}

fn error_ratio(shape: SerCount, disc: SerCount) -> f64 {
    let floored = SerCount {
        symbols: disc.symbols,
        errors: disc.errors.max(1),
    };
    if floored.symbols == 0 {
        1.0
    } else {
        shape.rate() / floored.rate()
    }
}

/// Progress callback: `(frames done, frames total)`, called from worker threads.
pub type Progress<'a> = &'a (dyn Fn(usize, usize) + Sync);

/// Generate, degrade and decode `cfg.frames` frames of `order` on all threads.
///
/// The result does not depend on the thread count: every frame is a pure function of
/// `(order, config, index)` and the sums commute.
pub fn run_shape(
    order: &DotOrder,
    shape_id: u8,
    table_version: u8,
    cfg: &ValidateConfig,
    progress: Progress<'_>,
) -> Result<RunStats, Error> {
    cfg.check()?;
    let frames = SyntheticFrames::new(order, shape_id, table_version, cfg.clone());
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let total = cfg.frames;
    let mut stats = RunStats::default();
    thread::scope(|scope| {
        let workers: Vec<_> = (0..cfg.thread_count())
            .map(|_| {
                scope.spawn(|| {
                    let mut local = RunStats::default();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= total {
                            break;
                        }
                        if let Some(frame) = frames.frame(i) {
                            local.add(&decode_frame(&frame, order));
                        }
                        progress(done.fetch_add(1, Ordering::Relaxed) + 1, total);
                    }
                    local
                })
            })
            .collect();
        for w in workers {
            match w.join() {
                Ok(local) => stats.merge(&local),
                Err(panic) => std::panic::resume_unwind(panic),
            }
        }
    });
    Ok(stats)
}

/// The two runs of rule 7 and what they say.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Comparison {
    /// The shape under test.
    pub shape: RunStats,
    /// The disc, under the same cameras.
    pub disc: RunStats,
    /// `judge(shape, disc)`.
    pub verdict: Verdict,
}

/// Run rule 7 for `shape` against the disc of `table`.
///
/// `progress` gets `(phase, done, total)` with phase `"shape"` then `"disc"`.
pub fn compare_with_disc(
    shape: &DotOrder,
    shape_id: u8,
    table: &ShapeTable,
    cfg: &ValidateConfig,
    progress: &(dyn Fn(&str, usize, usize) + Sync),
) -> Result<Comparison, Error> {
    let disc_order = table
        .dot_order(DISC, cfg.profile)
        .ok_or_else(|| Error::BadOption("table has no disc".into()))?;
    if shape.profile() != cfg.profile {
        return Err(Error::BadOption(
            "the dot order is for another profile".into(),
        ));
    }
    let shape_stats = run_shape(shape, shape_id, table.version(), cfg, &|d, t| {
        progress("shape", d, t);
    })?;
    let disc_stats = run_shape(&disc_order, DISC, table.version(), cfg, &|d, t| {
        progress("disc", d, t);
    })?;
    Ok(Comparison {
        shape: shape_stats,
        disc: disc_stats,
        verdict: judge_runs(&shape_stats, &disc_stats),
    })
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
            threads: 2,
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
        assert_eq!(a0.spec.header.shape_id, 30);
        assert!(matches!(a0.chain[0], Degradation::Tilt { .. }));
        assert_eq!(gen_a.count(), 2);
        // The truth really is a codeword: decoding the undamaged symbols returns the data.
        let codec = BodyCodec::new(order.len(), EccLevel::Normal);
        assert_eq!(
            codec.decode(&a0.spec.body_symbols(), a0.key).unwrap(),
            a0.data
        );
        assert_eq!(
            Header::decode(&a0.spec.header_symbols()).unwrap(),
            a0.spec.header
        );
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
        // A flawless disc only lets a flawless shape pass.
        let clean = SerCount {
            symbols: 10_000,
            errors: 0,
        };
        assert!(judge(clean, clean).passed);
        assert!(!judge(ok, clean).passed);
    }

    #[test]
    fn run_stats_rates() {
        let mut s = RunStats::default();
        s.add(&FrameOutcome {
            located: true,
            read: true,
            symbols: 100,
            errors: 5,
            read_symbols: 100,
            erasures: 10,
            header_decoded: true,
            body_decoded: true,
        });
        s.add(&FrameOutcome {
            symbols: 100,
            errors: 100,
            ..FrameOutcome::default()
        });
        assert!((s.ser() - 0.525).abs() < 1e-12);
        assert!((s.lock_rate() - 0.5).abs() < 1e-12);
        assert!((s.erasure_rate() - 0.1).abs() < 1e-12);
        assert!((s.decode_rate() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn clean_enough_frames_decode() {
        let table = ShapeTable::builtin();
        let order = table.dot_order(0, Profile::M).unwrap();
        let cfg = ValidateConfig {
            profile: Profile::M,
            frames: 2,
            ..ValidateConfig::default()
        };
        let stats = run_shape(&order, 0, 0, &cfg, &|_, _| {}).unwrap();
        assert_eq!(stats.frames, 2);
        assert_eq!(stats.located, 2, "{stats:?}");
        assert!(stats.ser() < 0.05, "{stats:?}");
        assert_eq!(stats.fully_decoded, 2, "{stats:?}");
    }

    #[test]
    fn thread_count_does_not_change_the_result() {
        let table = ShapeTable::builtin();
        let order = table.dot_order(30, Profile::S).unwrap();
        let one = ValidateConfig {
            threads: 1,
            ..small_cfg(4)
        };
        let three = ValidateConfig {
            threads: 3,
            ..small_cfg(4)
        };
        let a = run_shape(&order, 30, 0, &one, &|_, _| {}).unwrap();
        let b = run_shape(&order, 30, 0, &three, &|_, _| {}).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn rejects_bad_config() {
        let table = ShapeTable::builtin();
        let order = table.dot_order(0, Profile::S).unwrap();
        let cfg = ValidateConfig {
            render_px: 0,
            ..small_cfg(1)
        };
        assert!(run_shape(&order, 0, 0, &cfg, &|_, _| {}).is_err());
        let wrong_profile = ValidateConfig {
            profile: Profile::M,
            ..small_cfg(1)
        };
        assert!(compare_with_disc(&order, 0, &table, &wrong_profile, &|_, _, _| {}).is_err());
    }
}
