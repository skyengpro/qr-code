//! Animated GIF packager.

use std::collections::HashMap;

use gif::{Encoder, Frame, Repeat};
use particlecode_core::FrameSpec;
use particlecode_core::motion::{self, MotionConfig};
use particlecode_core::profile::{Rgb, colors};
use tiny_skia::Pixmap;

use crate::color::lerp_rgb;
use crate::config::RenderConfig;
use crate::error::RenderError;
use crate::paint::{Anim, render_anim};

/// Extra knobs of [`write_gif_with`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GifOptions {
    /// Rendered GIF frames per data frame (`>= 1`). With more than one, the motion
    /// functions are sampled at `i / fps + s / (n * fps)`, which bakes the drift into the
    /// file. Capped so that every GIF frame lasts at least one centisecond.
    pub subframes: u32,
    /// Insert a 20 ms mid-fade frame at every data-frame switch (needs a frame delay of at
    /// least 30 ms) so the colour cross-fade is visible in players that honour 2 cs delays.
    pub crossfade: bool,
}

impl Default for GifOptions {
    fn default() -> Self {
        GifOptions {
            subframes: 1,
            crossfade: false,
        }
    }
}

/// The fixed palette used for GIFs: the background, the anchor, white, black and particle
/// colours, the four hues, and 16-step blends from the background to every ink colour
/// (the anti-aliased edge of a dot), plus short hue-to-white ramps for the ripple. All flat
/// colours therefore survive quantisation exactly. At most 256 entries.
pub fn gif_palette(background: Rgb) -> Vec<Rgb> {
    let inks: Vec<Rgb> = colors::HUES4
        .into_iter()
        .chain([colors::ANCHOR, colors::PARTICLE, colors::BLACK])
        .collect();
    let mut p = vec![
        background,
        colors::ANCHOR,
        colors::WHITE,
        colors::BLACK,
        colors::PARTICLE,
        colors::HUES4[0],
        colors::HUES4[1],
        colors::HUES4[2],
        colors::HUES4[3],
    ];
    for &ink in &inks {
        for i in 1..16 {
            p.push(lerp_rgb(background, ink, f64::from(i) / 16.0));
        }
    }
    for i in 1..8 {
        p.push(lerp_rgb(background, colors::WHITE, f64::from(i) / 8.0));
    }
    for &hue in &colors::HUES4 {
        for step in [0.05, 0.10, 0.15] {
            p.push(lerp_rgb(hue, colors::WHITE, step));
        }
    }
    debug_assert!(p.len() <= 256);
    p
}

/// Map pixels to palette indices, remembering previous answers (frames are mostly flat
/// colour, so the cache hits almost always).
struct Quantizer {
    palette: Vec<Rgb>,
    cache: HashMap<u32, u8>,
}

impl Quantizer {
    fn new(palette: Vec<Rgb>) -> Quantizer {
        Quantizer {
            palette,
            cache: HashMap::new(),
        }
    }

    fn index(&mut self, r: u8, g: u8, b: u8) -> u8 {
        let key = u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b);
        if let Some(&i) = self.cache.get(&key) {
            return i;
        }
        let dist = |c: &Rgb| {
            let d = |a: u8, b: u8| i32::from(a) - i32::from(b);
            let (dr, dg, db) = (d(c.0, r), d(c.1, g), d(c.2, b));
            dr * dr + dg * dg + db * db
        };
        let best = self
            .palette
            .iter()
            .enumerate()
            .min_by_key(|(_, c)| dist(c))
            .map_or(0, |(i, _)| i as u8);
        self.cache.insert(key, best);
        best
    }

    fn quantize(&mut self, pm: &Pixmap) -> Vec<u8> {
        pm.pixels()
            .iter()
            .map(|p| {
                let c = p.demultiply();
                self.index(c.red(), c.green(), c.blue())
            })
            .collect()
    }
}

/// One GIF frame to render.
struct Slot {
    frame: usize,
    prev: Option<usize>,
    t: f64,
    t_since_switch: f64,
    t_since_start: Option<f64>,
    ripple: Option<f64>,
    delay_cs: u16,
}

/// Split `total` centiseconds over `n` parts, each at least 1.
fn split_delay(total: u16, n: u16) -> Vec<u16> {
    let n = n.clamp(1, total.max(1));
    let (base, extra) = (total / n, total % n);
    (0..n)
        .map(|i| (base + u16::from(i < extra)).max(1))
        .collect()
}

fn plan(
    frames: &[FrameSpec],
    cfg: &RenderConfig,
    motion: MotionConfig,
    opts: &GifOptions,
) -> Vec<Slot> {
    let fps = f64::from(cfg.fps);
    let cs = cfg.frame_delay_cs();
    let mut slots = Vec::new();

    // Reveal: the first data frame's dots fade in before any data frame counts as shown.
    let mut t0 = 0.0;
    if motion.reveal {
        let n = (motion::REVEAL_DURATION * fps - 1e-9).ceil() as usize;
        for j in 0..n {
            let t = j as f64 / fps;
            slots.push(Slot {
                frame: 0,
                prev: None,
                t,
                t_since_switch: f64::INFINITY,
                t_since_start: Some(t),
                ripple: None,
                delay_cs: cs,
            });
        }
        t0 = n as f64 / fps;
    }

    let mut ripple_start: Option<f64> = None;
    for (i, f) in frames.iter().enumerate() {
        let t_frame = t0 + i as f64 / fps;
        if i > 0 && f.header.segment_index != frames[i - 1].header.segment_index {
            ripple_start = Some(t_frame);
        }
        let mut parts = split_delay(cs, opts.subframes.clamp(1, u32::from(u16::MAX)) as u16);
        let mut sub_offset = 0.0;
        if opts.crossfade && i > 0 && cs >= 3 {
            slots.push(Slot {
                frame: i,
                prev: Some(i - 1),
                t: t_frame,
                t_since_switch: 0.02,
                t_since_start: None,
                ripple: ripple_start.map(|s| t_frame - s),
                delay_cs: 2,
            });
            parts = split_delay(cs - 2, parts.len() as u16);
            sub_offset = 0.02;
        }
        let mut elapsed = sub_offset;
        for d in parts {
            let t = t_frame + elapsed;
            slots.push(Slot {
                frame: i,
                prev: None,
                t,
                t_since_switch: f64::INFINITY,
                t_since_start: None,
                ripple: ripple_start.map(|s| t - s),
                delay_cs: d,
            });
            elapsed += f64::from(d) / 100.0;
        }
    }
    slots
}

/// Pack data frames into a looping animated GIF at `cfg.fps` (one GIF frame per data
/// frame). See [`write_gif_with`] for sub-frames and the baked cross-fade.
///
/// The motion layers in `motion` are baked in with the seed
/// [`motion::seed_from_session`]`(frames[0].header.session_id)`, so an emailed GIF moves
/// like the app does. With `motion.reveal` the file starts with a 600 ms fade-in of the
/// first frame; with `motion.ripple` a ring of brightness runs after every segment change.
pub fn write_gif(
    frames: &[FrameSpec],
    cfg: &RenderConfig,
    motion: &MotionConfig,
) -> Result<Vec<u8>, RenderError> {
    write_gif_with(frames, cfg, motion, &GifOptions::default())
}

/// [`write_gif`] with explicit [`GifOptions`].
///
/// Pixels are mapped to the fixed [`gif_palette`] by nearest colour, so flat dot colours
/// are exact and only the one-pixel anti-aliased edges are quantised.
pub fn write_gif_with(
    frames: &[FrameSpec],
    cfg: &RenderConfig,
    motion: &MotionConfig,
    opts: &GifOptions,
) -> Result<Vec<u8>, RenderError> {
    cfg.validate()?;
    let first = frames.first().ok_or(RenderError::NoFrames)?;
    let size = u16::try_from(cfg.size_px).map_err(|_| RenderError::BadSize(cfg.size_px))?;
    let seed = motion::seed_from_session(first.header.session_id);

    let palette = gif_palette(cfg.background);
    let mut flat = Vec::with_capacity(256 * 3);
    for c in &palette {
        flat.extend_from_slice(&[c.0, c.1, c.2]);
    }
    flat.resize(256 * 3, 0);

    let mut out = Vec::new();
    let mut enc = Encoder::new(&mut out, size, size, &flat)?;
    enc.set_repeat(Repeat::Infinite)?;
    let mut quant = Quantizer::new(palette);

    for slot in plan(frames, cfg, *motion, opts) {
        let anim = Anim {
            motion: *motion,
            seed,
            t: slot.t,
            prev: slot.prev.map(|p| &frames[p]),
            t_since_switch: slot.t_since_switch,
            t_since_segment_complete: slot.ripple.filter(|&r| motion.ripple && r >= 0.0),
            t_since_start: slot.t_since_start,
        };
        let pm = render_anim(&frames[slot.frame], cfg, &anim);
        let mut frame = Frame::from_indexed_pixels(size, size, quant.quantize(&pm), None);
        frame.delay = slot.delay_cs;
        enc.write_frame(&frame)?;
    }
    drop(enc);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_fits_and_contains_the_flat_colours() {
        let p = gif_palette(colors::BACKGROUND);
        assert!(p.len() <= 256, "{}", p.len());
        for c in colors::HUES4.into_iter().chain([
            colors::BACKGROUND,
            colors::ANCHOR,
            colors::WHITE,
            colors::BLACK,
            colors::PARTICLE,
        ]) {
            assert!(p.contains(&c), "{c:?}");
        }
    }

    #[test]
    fn quantizer_is_exact_on_palette_colours() {
        let mut q = Quantizer::new(gif_palette(colors::BACKGROUND));
        for c in colors::HUES4 {
            let i = q.index(c.0, c.1, c.2);
            assert_eq!(q.palette[usize::from(i)], c);
        }
    }

    #[test]
    fn delays_add_up() {
        assert_eq!(split_delay(10, 1), vec![10]);
        assert_eq!(split_delay(10, 3), vec![4, 3, 3]);
        assert_eq!(split_delay(2, 5), vec![1, 1]);
        for n in 1..12 {
            assert_eq!(split_delay(10, n).iter().sum::<u16>(), 10);
        }
    }
}
