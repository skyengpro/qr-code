//! The painter: [`FrameSpec`] to [`Pixmap`], static or with motion.

use particlecode_core::FrameSpec;
use particlecode_core::frame::{AnchorSpec, Dot, ParticleSpec};
use particlecode_core::layout::{Point, SwatchColor, dims};
use particlecode_core::motion::{self, MotionConfig};
use particlecode_core::profile::{Rgb, colors};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};

use crate::color::{lerp_hue, lerp_rgb, lighten};
use crate::config::RenderConfig;

/// Everything the painter needs besides the frame to draw a moment of an animation.
///
/// All times are seconds. The motion functions in [`particlecode_core::motion`] are pure
/// functions of these numbers, so the same `Anim` always yields the same pixels.
#[derive(Debug, Clone, Copy)]
pub struct Anim<'a> {
    /// Which layers are on.
    pub motion: MotionConfig,
    /// Session seed (see [`particlecode_core::motion::seed_from_session`]).
    pub seed: u64,
    /// Time since the session started; drives drift, heartbeat and particle wobble.
    pub t: f64,
    /// The data frame that was on screen before this one, for the colour cross-fade and
    /// the particle glide. `None` shows `frame` outright.
    pub prev: Option<&'a FrameSpec>,
    /// Time since the switch from `prev` to the current frame.
    pub t_since_switch: f64,
    /// Time since the last completed segment, for the ripple (`None` = no ripple now).
    pub t_since_segment_complete: Option<f64>,
    /// Time since the reveal started (`None` = fully revealed).
    pub t_since_start: Option<f64>,
}

impl<'a> Anim<'a> {
    /// An `Anim` at session time `t` with no switch, ripple or reveal in progress.
    pub fn at(motion: MotionConfig, seed: u64, t: f64) -> Anim<'a> {
        Anim {
            motion,
            seed,
            t,
            prev: None,
            t_since_switch: f64::INFINITY,
            t_since_segment_complete: None,
            t_since_start: None,
        }
    }
}

/// Render one frame without motion: dots at their lattice points, particles at their
/// encoded angles. `cfg.size_px` is clamped to `1..=`[`RenderConfig::MAX_SIZE_PX`].
pub fn render(frame: &FrameSpec, cfg: &RenderConfig) -> Pixmap {
    paint(frame, cfg, None)
}

/// Render one frame at session time `t_in_session` with the layers of `motion`.
///
/// `prev` is the data frame shown before this one and `t_since_switch` the time since the
/// switch: dots that change colour cross-fade over 40 ms and particles glide to their new
/// sectors. When `prev` belongs to a different segment the ripple runs from the switch;
/// when `motion.reveal` is on the dots fade in over the first 600 ms of the session. Use
/// [`render_anim`] to control the ripple and reveal clocks yourself.
pub fn render_animated(
    frame: &FrameSpec,
    prev: Option<&FrameSpec>,
    cfg: &RenderConfig,
    motion: &MotionConfig,
    seed: u64,
    t_in_session: f64,
    t_since_switch: f64,
) -> Pixmap {
    let segment_changed =
        prev.is_some_and(|p| p.header.segment_index != frame.header.segment_index);
    let anim = Anim {
        motion: *motion,
        seed,
        t: t_in_session,
        prev,
        t_since_switch,
        t_since_segment_complete: segment_changed.then_some(t_since_switch),
        t_since_start: motion.reveal.then_some(t_in_session),
    };
    paint(frame, cfg, Some(&anim))
}

/// Render one frame with fully explicit animation state.
pub fn render_anim(frame: &FrameSpec, cfg: &RenderConfig, anim: &Anim<'_>) -> Pixmap {
    paint(frame, cfg, Some(anim))
}

/// Number of fading discs in a particle trail.
const TRAIL_STEPS: usize = 8;

/// Pixel canvas with pitch-unit drawing helpers.
struct Canvas {
    pm: Pixmap,
    paint: Paint<'static>,
    scale: f64,
    half: f64,
}

impl Canvas {
    fn new(frame: &FrameSpec, cfg: &RenderConfig) -> Canvas {
        let size = cfg.size_px.clamp(1, RenderConfig::MAX_SIZE_PX);
        let mut pm = Pixmap::new(size, size).expect("size is clamped to a valid, non-zero range");
        pm.fill(color(cfg.background, 1.0));
        // `Paint::default()` is anti-aliased.
        let paint = Paint::default();
        Canvas {
            pm,
            paint,
            scale: cfg.pitch_px(frame),
            half: cfg.half_extent(frame),
        }
    }

    fn px(&self, v: f64) -> f32 {
        ((v + self.half) * self.scale) as f32
    }

    fn set_color(&mut self, c: Rgb, alpha: f64) {
        self.paint.set_color(color(c, alpha));
    }

    /// Filled circle of radius `r` pitches. Exact flat colour, anti-aliased edge.
    fn disc(&mut self, at: Point, r: f64, c: Rgb, alpha: f64) {
        if r <= 0.0 || alpha <= 0.0 {
            return;
        }
        let Some(path) =
            PathBuilder::from_circle(self.px(at.x), self.px(at.y), (r * self.scale) as f32)
        else {
            return;
        };
        self.set_color(c, alpha);
        self.pm.fill_path(
            &path,
            &self.paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    /// Annulus between `r_in` and `r_out` pitches (even-odd fill, so the hole is clear).
    fn ring(&mut self, at: Point, r_in: f64, r_out: f64, c: Rgb) {
        if r_out <= r_in || r_out <= 0.0 {
            return;
        }
        let (cx, cy) = (self.px(at.x), self.px(at.y));
        let mut pb = PathBuilder::new();
        pb.push_circle(cx, cy, (r_out * self.scale) as f32);
        pb.push_circle(cx, cy, (r_in.max(0.0) * self.scale) as f32);
        let Some(path) = pb.finish() else {
            return;
        };
        self.set_color(c, 1.0);
        self.pm.fill_path(
            &path,
            &self.paint,
            FillRule::EvenOdd,
            Transform::identity(),
            None,
        );
    }
}

fn color(c: Rgb, alpha: f64) -> Color {
    Color::from_rgba8(c.0, c.1, c.2, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn swatch_rgb(c: SwatchColor) -> Rgb {
    match c {
        SwatchColor::Hue(h) => colors::HUES4[usize::from(h & 3)],
        SwatchColor::White => colors::WHITE,
        SwatchColor::Black => colors::BLACK,
    }
}

fn paint(frame: &FrameSpec, cfg: &RenderConfig, anim: Option<&Anim<'_>>) -> Pixmap {
    let mut cv = Canvas::new(frame, cfg);
    draw_anchors(&mut cv, frame, cfg, anim);
    draw_swatches(&mut cv, frame, anim);
    draw_dots(
        &mut cv,
        frame,
        &frame.header_dots,
        prev_dots(frame, anim, |p| &p.header_dots),
        anim,
        false,
    );
    draw_dots(
        &mut cv,
        frame,
        &frame.dots,
        prev_dots(frame, anim, |p| &p.dots),
        anim,
        true,
    );
    draw_particles(&mut cv, frame, anim);
    cv.pm
}

/// The previous frame's dot list if it is comparable dot-for-dot.
fn prev_dots<'a>(
    frame: &FrameSpec,
    anim: Option<&Anim<'a>>,
    pick: impl Fn(&'a FrameSpec) -> &'a [Dot],
) -> Option<&'a [Dot]> {
    let prev = anim?.prev?;
    (prev.profile == frame.profile).then(|| pick(prev))
}

fn draw_anchors(cv: &mut Canvas, frame: &FrameSpec, cfg: &RenderConfig, anim: Option<&Anim<'_>>) {
    for (i, a) in frame.anchors.iter().enumerate() {
        let s = anim.map_or(1.0, |an| motion::anchor_scale(&an.motion, an.t, i));
        draw_anchor(cv, a, s, cfg.background);
    }
}

fn draw_anchor(cv: &mut Canvas, a: &AnchorSpec, scale: f64, pupil_color: Rgb) {
    cv.ring(a.at, a.ring_in * scale, a.ring_out * scale, colors::ANCHOR);
    cv.disc(a.at, a.core_r * scale, colors::ANCHOR, 1.0);
    if a.pupil_r > 0.0 {
        cv.disc(a.at, a.pupil_r * scale, pupil_color, 1.0);
    }
}

/// Opacity of a (non-anchor) dot from the reveal.
fn reveal_alpha(frame: &FrameSpec, at: Point, anim: Option<&Anim<'_>>) -> f64 {
    match anim {
        Some(an) if an.motion.reveal => an.t_since_start.map_or(1.0, |ts| {
            motion::reveal_opacity(ts, at, frame.frame_half_side)
        }),
        _ => 1.0,
    }
}

/// Calibration swatches stay on their rings: the receiver reads colour *and* size
/// references from them, so only the reveal touches them.
fn draw_swatches(cv: &mut Canvas, frame: &FrameSpec, anim: Option<&Anim<'_>>) {
    for s in &frame.swatches {
        let alpha = reveal_alpha(frame, s.at, anim);
        cv.disc(s.at, dims::SWATCH_DOT_R, swatch_rgb(s.color), alpha);
    }
}

/// Draw header dots (`lattice == false`: rigid, no drift or pulse, because their size
/// references are the static swatches) or payload dots (`lattice == true`: drift, shared
/// radius pulse, ripple).
fn draw_dots(
    cv: &mut Canvas,
    frame: &FrameSpec,
    dots: &[Dot],
    prev: Option<&[Dot]>,
    anim: Option<&Anim<'_>>,
    lattice: bool,
) {
    let r_half = frame.frame_half_side;
    let fade = anim
        .filter(|a| a.prev.is_some())
        .map_or(1.0, |a| motion::crossfade_alpha(a.t_since_switch));
    // A prev list is only comparable if it has the same shape.
    let prev = prev.filter(|p| p.len() == dots.len());
    for (i, d) in dots.iter().enumerate() {
        let mut rgb = colors::HUES4[usize::from(d.color & 3)];
        let mut radius = d.radius();
        if fade < 1.0
            && let Some(p) = prev.map(|p| &p[i]).filter(|p| p.q == d.q && p.r == d.r)
        {
            let from = colors::HUES4[usize::from(p.color & 3)];
            let shimmer = anim.is_some_and(|a| a.motion.shimmer);
            rgb = if shimmer {
                lerp_hue(from, rgb, fade)
            } else {
                lerp_rgb(from, rgb, fade)
            };
            radius = p.radius() + (radius - p.radius()) * fade;
        }
        let mut at = d.at;
        let mut alpha = 1.0;
        if let Some(an) = anim {
            if lattice {
                let off = motion::dot_offset(&an.motion, an.seed, an.t, d.at, r_half);
                at = Point::new(d.at.x + off.x, d.at.y + off.y);
                radius *= motion::dot_radius_scale(&an.motion, an.seed, an.t, d.at, r_half);
                if an.motion.ripple
                    && let Some(ts) = an.t_since_segment_complete
                {
                    rgb = lighten(rgb, motion::ripple_lighten(ts, d.at, r_half));
                }
            }
            alpha = reveal_alpha(frame, d.at, anim);
        }
        cv.disc(at, radius, rgb, alpha);
    }
}

fn draw_particles(cv: &mut Canvas, frame: &FrameSpec, anim: Option<&Anim<'_>>) {
    let band = &frame.particle_band;
    for (k, p) in frame.particles.iter().enumerate() {
        let Some(an) = anim else {
            cv.disc(
                Point::default().polar(band.mid(), p.angle),
                dims::PARTICLE_R,
                colors::PARTICLE,
                1.0,
            );
            continue;
        };
        let from = an
            .prev
            .and_then(|pf| pf.particles.get(k))
            .map_or(p.angle, |q: &ParticleSpec| q.angle);
        let pose = |dt: f64| {
            motion::particle_pose(
                an.seed,
                an.t - dt,
                k,
                from,
                p.angle,
                an.t_since_switch - dt,
                band,
            )
        };
        let (angle, radius) = pose(0.0);
        if an.motion.particle_trails {
            // The tail lies behind the direction of travel, along the arc.
            let (before, _) = pose(0.02);
            let sign = if angle >= before { 1.0 } else { -1.0 };
            let span = motion::TRAIL_LENGTH / radius.max(1.0);
            for s in (1..=TRAIL_STEPS).rev() {
                let f = s as f64 / TRAIL_STEPS as f64;
                let a = angle - sign * span * f;
                let at = Point::default().polar(radius, a);
                cv.disc(
                    at,
                    dims::PARTICLE_R * (1.0 - 0.45 * f),
                    colors::PARTICLE,
                    0.55 * (1.0 - f),
                );
            }
        }
        cv.disc(
            Point::default().polar(radius, angle),
            dims::PARTICLE_R,
            colors::PARTICLE,
            1.0,
        );
    }
}
