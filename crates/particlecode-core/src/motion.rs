//! Animation "moods": deterministic functions of (t, session seed, transfer progress) that
//! every painter (Flutter, web, the Rust GIF packager) evaluates identically.
//!
//! Everything here is a pure function of its arguments: nothing is accumulated, so a
//! dropped render frame never drifts the field. Times are in seconds, lengths in dot
//! pitches `p` (see [`crate::layout`]). The decoder samples a disc of
//! [`dims::SAMPLE_R`] around each lattice point, so the bounds below are what keeps the
//! animation invisible to it:
//!
//! * dot drift is at most [`MAX_DRIFT`] (0.12 p), below the format's hard limit
//!   [`dims::MAX_DRIFT`];
//! * dot radius pulses by at most ±[`RADIUS_PULSE`] and the pulse is spatially smooth, so
//!   neighbouring dots share it;
//! * nothing here ever changes a hue by more than the optional ripple's 15% lightening.
//!
//! A painter checks the relevant [`MotionConfig`] toggle before it applies a function:
//! [`ripple_lighten`], [`reveal_opacity`] and [`crossfade_alpha`] are the raw curves and
//! do not look at the config; [`dot_offset`], [`dot_radius_scale`] and [`anchor_scale`]
//! do.

use core::f64::consts::{FRAC_1_SQRT_2, PI};

use crate::layout::{ParticleBand, Point, dims};

/// Largest distance a dot may drift from its lattice point, in pitches (spec: 0.12 p).
pub const MAX_DRIFT: f64 = 0.12;
/// Drift amplitude actually used: a hair below [`MAX_DRIFT`] so float rounding can never
/// cross it.
const DRIFT_AMPLITUDE: f64 = 0.115;
/// Relative dot radius pulse (±5%).
pub const RADIUS_PULSE: f64 = 0.05;
/// Period of the breathing wave in seconds.
pub const BREATH_PERIOD: f64 = 2.5;
/// Wavelength of the breathing wave as a fraction of the frame half-side `R`.
pub const BREATH_WAVELENGTH_FRAC: f64 = 0.6;
/// Largest extra scale of a hero anchor ring under the heartbeat (+10%).
pub const HEARTBEAT_SWELL: f64 = 0.10;
/// Period of the anchor heartbeat in seconds.
pub const HEARTBEAT_PERIOD: f64 = 3.0;
/// Peak lightening of a dot on the ripple ring (15%).
pub const RIPPLE_STRENGTH: f64 = 0.15;
/// Duration of the ripple in seconds.
pub const RIPPLE_DURATION: f64 = 0.3;
/// Duration of the reveal in seconds.
pub const REVEAL_DURATION: f64 = 0.6;
/// Duration of the colour cross-fade at a data-frame switch, in seconds.
pub const CROSSFADE_DURATION: f64 = 0.04;
/// Duration of the particle glide at a data-frame switch, in seconds.
pub const PARTICLE_GLIDE: f64 = 0.06;
/// Largest particle wobble around its sector centre, in radians (0.8°).
pub const PARTICLE_WOBBLE: f64 = 0.8 * PI / 180.0;
/// Length of a particle trail, in pitches (spec: 0.8 p).
pub const TRAIL_LENGTH: f64 = 0.8;

/// How dots drift around their lattice points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Mood {
    /// A slow sine wave travelling across the field (the default).
    #[default]
    Breathing,
    /// A smooth seeded noise field: wind through grass.
    FlowField,
}

/// Which animation layers a painter applies. [`MotionConfig::default`] is the spec's
/// default: the breathing wave and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(clippy::struct_excessive_bools)] // independent on/off layers, one flag each
pub struct MotionConfig {
    /// Dot drift and radius pulse.
    pub mood: Mood,
    /// A ring of brightness on every completed segment.
    pub ripple: bool,
    /// The three hero rings swell in a slow, offset pulse.
    pub anchor_heartbeat: bool,
    /// Particles leave a fading tail.
    pub particle_trails: bool,
    /// Dots fade in from the centre at session start.
    pub reveal: bool,
    /// Hue-rotating (instead of straight RGB) cross-fade at a data-frame switch.
    pub shimmer: bool,
}

impl MotionConfig {
    /// Every optional layer switched on, breathing mood.
    pub const fn all() -> MotionConfig {
        MotionConfig {
            mood: Mood::Breathing,
            ripple: true,
            anchor_heartbeat: true,
            particle_trails: true,
            reveal: true,
            shimmer: true,
        }
    }
}

// ---------------------------------------------------------------------------------------
// Hashing and noise
// ---------------------------------------------------------------------------------------

/// `SplitMix64` finaliser: a good 64-bit mixer, identical on every platform.
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Uniform value in `[0, 1)` derived from `(seed, salt)`.
fn unit(seed: u64, salt: u64) -> f64 {
    (mix(seed ^ mix(salt)) >> 11) as f64 / (1u64 << 53) as f64
}

/// Derive the motion seed every painter should use from a session id, so that the app,
/// the GIF packager and the web painter agree on direction and phase.
pub fn seed_from_session(session_id: u16) -> u64 {
    mix(0x5043_4F44_4500_0000 | u64::from(session_id))
}

fn smoothstep(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// 2-D simplex noise with hashed gradients, range roughly `[-1, 1]`. Smooth (C¹) in both
/// arguments.
fn simplex2(seed: u64, x: f64, y: f64) -> f64 {
    const F2: f64 = 0.366_025_403_784_438_6; // (√3 − 1) / 2
    const G2: f64 = 0.211_324_865_405_187_1; // (3 − √3) / 6
    // Eight unit-ish gradient directions.
    const GRAD: [(f64, f64); 8] = [
        (1.0, 0.0),
        (FRAC_1_SQRT_2, FRAC_1_SQRT_2),
        (0.0, 1.0),
        (-FRAC_1_SQRT_2, FRAC_1_SQRT_2),
        (-1.0, 0.0),
        (-FRAC_1_SQRT_2, -FRAC_1_SQRT_2),
        (0.0, -1.0),
        (FRAC_1_SQRT_2, -FRAC_1_SQRT_2),
    ];

    let s = (x + y) * F2;
    let i = libm::floor(x + s);
    let j = libm::floor(y + s);
    let t = (i + j) * G2;
    let x0 = x - (i - t);
    let y0 = y - (j - t);
    let (i1, j1) = if x0 > y0 { (1.0, 0.0) } else { (0.0, 1.0) };
    let x1 = x0 - i1 + G2;
    let y1 = y0 - j1 + G2;
    let x2 = x0 - 1.0 + 2.0 * G2;
    let y2 = y0 - 1.0 + 2.0 * G2;

    let corner = |ci: f64, cj: f64, dx: f64, dy: f64| {
        let falloff = 0.5 - dx * dx - dy * dy;
        if falloff <= 0.0 {
            return 0.0;
        }
        let h = mix(seed ^ mix((ci as i64 as u64) ^ mix(cj as i64 as u64)));
        let (gx, gy) = GRAD[(h & 7) as usize];
        let f2 = falloff * falloff;
        f2 * f2 * (gx * dx + gy * dy)
    };
    let n =
        corner(i, j, x0, y0) + corner(i + i1, j + j1, x1, y1) + corner(i + 1.0, j + 1.0, x2, y2);
    // 70 normalises the sum to about [-1, 1] for these gradients.
    (70.0 * n).clamp(-1.0, 1.0)
}

// ---------------------------------------------------------------------------------------
// Dot drift and pulse
// ---------------------------------------------------------------------------------------

/// Unit travel direction and start phase of the breathing wave for a seed.
fn wave_params(seed: u64) -> (f64, f64, f64) {
    let theta = unit(seed, 1) * 2.0 * PI;
    let phase0 = unit(seed, 2) * 2.0 * PI;
    (libm::cos(theta), libm::sin(theta), phase0)
}

/// Phase of the breathing wave at point `at` and time `t`.
fn wave_phase(seed: u64, t: f64, at: Point, half_side: f64) -> (f64, f64, f64) {
    let (dx, dy, phase0) = wave_params(seed);
    let k = 2.0 * PI / (BREATH_WAVELENGTH_FRAC * half_side.max(1.0));
    let phase = k * (at.x * dx + at.y * dy) - 2.0 * PI * t / BREATH_PERIOD + phase0;
    (phase, dx, dy)
}

/// Scale and drift parameters of the flow field, in pitch units per noise unit.
fn flow_coords(seed: u64, t: f64, at: Point, half_side: f64) -> (f64, f64) {
    let feature = 0.6 * half_side.max(1.0); // size of one noise cell
    let wind = half_side.max(1.0) / 7.0; // pitches per second the field is advected
    let (dx, dy, _) = wave_params(seed);
    (
        (at.x - dx * wind * t) / feature,
        (at.y - dy * wind * t) / feature,
    )
}

/// Offset of a dot from its lattice point, in pitches. `|offset| ≤ 0.12` always.
///
/// `at` is the rest position, `half_side` the frame half-side `R`
/// ([`crate::frame::FrameSpec::frame_half_side`]). Breathing is a travelling sine of
/// period 2.5 s and wavelength `0.6 R`, with direction and phase taken from `seed`; the
/// flow field is seeded simplex noise advected slowly across the frame.
pub fn dot_offset(cfg: &MotionConfig, seed: u64, t: f64, at: Point, half_side: f64) -> Point {
    match cfg.mood {
        Mood::Breathing => {
            let (phase, dx, dy) = wave_phase(seed, t, at, half_side);
            // Elongated along the travel direction, a little sideways: |v| ≤ 1.
            let along = libm::sin(phase);
            let across = 0.5 * libm::cos(phase);
            Point::new(
                DRIFT_AMPLITUDE * (along * dx - across * dy),
                DRIFT_AMPLITUDE * (along * dy + across * dx),
            )
        }
        Mood::FlowField => {
            let (u, v) = flow_coords(seed, t, at, half_side);
            let nx = simplex2(seed ^ 0x11, u, v);
            let ny = simplex2(seed ^ 0x22, u + 31.7, v - 17.3);
            // Soft norm clamp keeps |v| ≤ 1 without a kink.
            let n = libm::hypot(nx, ny).max(1.0);
            Point::new(DRIFT_AMPLITUDE * nx / n, DRIFT_AMPLITUDE * ny / n)
        }
    }
}

/// Multiplier for a dot's radius at `at`, in `[0.95, 1.05]`. Spatially smooth over a
/// wavelength of several tens of pitches, so neighbours share it and a classifier that
/// compares a dot to its neighbours does not notice.
pub fn dot_radius_scale(cfg: &MotionConfig, seed: u64, t: f64, at: Point, half_side: f64) -> f64 {
    let s = match cfg.mood {
        Mood::Breathing => libm::sin(wave_phase(seed, t, at, half_side).0),
        Mood::FlowField => {
            let (u, v) = flow_coords(seed, t, at, half_side);
            simplex2(seed ^ 0x33, u, v)
        }
    };
    1.0 + RADIUS_PULSE * s.clamp(-1.0, 1.0)
}

/// Scale of hero anchor `anchor_index` (0..3; index 3 is the centre mark) at time `t`.
/// `1.0` unless the heartbeat is on; never above `1.10`. The three pulses are offset by a
/// third of the period.
pub fn anchor_scale(cfg: &MotionConfig, t: f64, anchor_index: usize) -> f64 {
    if !cfg.anchor_heartbeat || anchor_index >= 3 {
        return 1.0;
    }
    let cycle = t / HEARTBEAT_PERIOD - anchor_index as f64 / 3.0;
    let beat = 0.5 - 0.5 * libm::cos(2.0 * PI * cycle); // 0..1
    // Squaring gives a short swell and a long rest.
    1.0 + HEARTBEAT_SWELL * beat * beat
}

// ---------------------------------------------------------------------------------------
// Event-driven curves
// ---------------------------------------------------------------------------------------

/// How much a dot at `at` lightens (`0..=0.15`, as a fraction of the way to white)
/// `t_since_segment_complete` seconds after a segment completed. A ring of brightness
/// expands from the centre to the frame edge over 300 ms; zero before and after.
pub fn ripple_lighten(t_since_segment_complete: f64, at: Point, half_side: f64) -> f64 {
    let u = t_since_segment_complete / RIPPLE_DURATION;
    if !(0.0..1.0).contains(&u) {
        return 0.0;
    }
    let ring = u * half_side * 1.05;
    let width = 0.18 * half_side.max(1.0);
    let x = (at.norm() - ring) / width; // -1..1 inside the ring
    if x.abs() >= 1.0 {
        return 0.0;
    }
    let bump = 0.5 + 0.5 * libm::cos(PI * x);
    let envelope = libm::sin(PI * u);
    (RIPPLE_STRENGTH * bump * envelope).clamp(0.0, RIPPLE_STRENGTH)
}

/// Opacity of a dot at `at`, `t_since_start` seconds into the session: dots fade in from
/// the centre outwards and are all fully opaque from [`REVEAL_DURATION`] (600 ms) on.
pub fn reveal_opacity(t_since_start: f64, at: Point, half_side: f64) -> f64 {
    if t_since_start >= REVEAL_DURATION {
        return 1.0;
    }
    let s = t_since_start / REVEAL_DURATION;
    let d = (at.norm() / half_side.max(1.0)).clamp(0.0, 1.0);
    // Each dot fades over 40% of the reveal, starting later the farther it is out.
    smoothstep((s - 0.6 * d) / 0.4)
}

/// Opacity of the *new* data frame `t_since_switch` seconds after the switch: `0` at the
/// switch, `1` from 40 ms on.
pub fn crossfade_alpha(t_since_switch: f64) -> f64 {
    (t_since_switch / CROSSFADE_DURATION).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------------------
// Particles
// ---------------------------------------------------------------------------------------

/// Pose `(angle, radius)` of particle `k` around the frame centre, in radians and
/// pitches.
///
/// Within [`PARTICLE_GLIDE`] of a switch the particle glides from the centre of its old
/// sector (`from_angle`) to the new one (`to_angle`); otherwise it wobbles by less than
/// ±0.8° around `to_angle` (a fifth of a sector) and drifts radially inside `band`,
/// always keeping the whole speck inside it.
pub fn particle_pose(
    seed: u64,
    t: f64,
    k: usize,
    from_angle: f64,
    to_angle: f64,
    t_since_switch: f64,
    band: &ParticleBand,
) -> (f64, f64) {
    let ks = k as u64 + 1;
    let f1 = 0.31 + 0.17 * unit(seed, 100 + ks);
    let f2 = 0.53 + 0.23 * unit(seed, 200 + ks);
    let p1 = unit(seed, 300 + ks) * 2.0 * PI;
    let p2 = unit(seed, 400 + ks) * 2.0 * PI;
    let wobble = PARTICLE_WOBBLE
        * (0.6 * libm::sin(2.0 * PI * f1 * t + p1) + 0.38 * libm::sin(2.0 * PI * f2 * t + p2));

    let glide = smoothstep(t_since_switch / PARTICLE_GLIDE);
    let centre = from_angle + (to_angle - from_angle) * glide;
    let angle = centre + wobble;

    let half = (0.5 * (band.outer - band.inner) - dims::PARTICLE_R).max(0.0);
    let g1 = 0.11 + 0.09 * unit(seed, 500 + ks);
    let g2 = 0.27 + 0.13 * unit(seed, 600 + ks);
    let r = 0.65 * libm::sin(2.0 * PI * g1 * t + unit(seed, 700 + ks) * 2.0 * PI)
        + 0.35 * libm::sin(2.0 * PI * g2 * t + unit(seed, 800 + ks) * 2.0 * PI);
    (angle, band.mid() + half * r.clamp(-1.0, 1.0))
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // determinism tests compare bit-identical results
mod tests {
    use super::*;

    /// Tiny deterministic generator for tests.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    const MOODS: [Mood; 2] = [Mood::Breathing, Mood::FlowField];

    fn cfg(mood: Mood) -> MotionConfig {
        MotionConfig {
            mood,
            ..MotionConfig::all()
        }
    }

    #[test]
    fn drift_is_bounded_and_deterministic() {
        let mut rng = Lcg(7);
        for mood in MOODS {
            let c = cfg(mood);
            for _ in 0..10_000 {
                let seed = (rng.next() * 1e9) as u64;
                let t = rng.next() * 600.0;
                let r = 20.0 + rng.next() * 40.0;
                let at = Point::new((rng.next() - 0.5) * 2.0 * r, (rng.next() - 0.5) * 2.0 * r);
                let o = dot_offset(&c, seed, t, at, r);
                assert!(o.norm() <= MAX_DRIFT, "{mood:?}: {}", o.norm());
                assert!(o.norm() <= dims::MAX_DRIFT);
                assert_eq!(o, dot_offset(&c, seed, t, at, r));
                let s = dot_radius_scale(&c, seed, t, at, r);
                assert!(
                    (1.0 - RADIUS_PULSE..=1.0 + RADIUS_PULSE).contains(&s),
                    "{s}"
                );
            }
        }
    }

    #[test]
    fn drift_actually_moves_and_depends_on_seed() {
        for mood in MOODS {
            let c = cfg(mood);
            let at = Point::new(3.0, -4.0);
            let a = dot_offset(&c, 1, 0.0, at, 45.0);
            let b = dot_offset(&c, 1, 0.7, at, 45.0);
            let d = dot_offset(&c, 2, 0.0, at, 45.0);
            assert!(a.dist(b) > 1e-3, "{mood:?} static in time");
            assert!(a.dist(d) > 1e-3, "{mood:?} ignores the seed");
        }
    }

    #[test]
    fn breathing_has_the_specified_period() {
        let c = MotionConfig::default();
        let at = Point::new(10.0, 5.0);
        let a = dot_offset(&c, 9, 0.3, at, 45.0);
        let b = dot_offset(&c, 9, 0.3 + BREATH_PERIOD, at, 45.0);
        assert!(a.dist(b) < 1e-9);
    }

    #[test]
    fn neighbours_move_together() {
        // Adjacent lattice points differ by far less than the drift amplitude, so the
        // classifier's neighbour comparison and the sampling disc are unaffected.
        let mut rng = Lcg(3);
        for mood in MOODS {
            let c = cfg(mood);
            for _ in 0..2000 {
                let r = 45.0;
                let at = Point::new((rng.next() - 0.5) * 2.0 * r, (rng.next() - 0.5) * 2.0 * r);
                let nb = Point::new(at.x + 1.0, at.y);
                let t = rng.next() * 30.0;
                let (a, b) = (dot_offset(&c, 5, t, at, r), dot_offset(&c, 5, t, nb, r));
                assert!(a.dist(b) < 0.03, "{mood:?}: {}", a.dist(b));
                let (sa, sb) = (
                    dot_radius_scale(&c, 5, t, at, r),
                    dot_radius_scale(&c, 5, t, nb, r),
                );
                assert!((sa - sb).abs() < 0.01, "{mood:?}: {}", (sa - sb).abs());
            }
        }
    }

    #[test]
    fn flow_field_is_smooth_in_time() {
        let c = cfg(Mood::FlowField);
        let at = Point::new(-7.0, 12.0);
        let mut prev = dot_offset(&c, 4, 0.0, at, 45.0);
        for i in 1..=500 {
            let cur = dot_offset(&c, 4, f64::from(i) * 0.01, at, 45.0);
            assert!(cur.dist(prev) < 0.01, "jump at step {i}");
            prev = cur;
        }
    }

    #[test]
    fn anchor_heartbeat_is_bounded_and_off_by_default() {
        let on = cfg(Mood::Breathing);
        let off = MotionConfig::default();
        let mut max: f64 = 1.0;
        for i in 0..3000 {
            let t = f64::from(i) * 0.01;
            for a in 0..4 {
                let s = anchor_scale(&on, t, a);
                assert!((1.0..=1.0 + HEARTBEAT_SWELL + 1e-12).contains(&s));
                max = max.max(s);
                assert_eq!(anchor_scale(&off, t, a), 1.0);
            }
        }
        assert!(max > 1.09, "heartbeat never swells: {max}");
        // The three pulses are offset.
        assert!((anchor_scale(&on, 0.0, 0) - anchor_scale(&on, 0.0, 1)).abs() > 1e-3);
    }

    #[test]
    fn ripple_is_a_bounded_expanding_ring() {
        let r = 45.0;
        let mut peak: f64 = 0.0;
        let mut rng = Lcg(11);
        for _ in 0..5000 {
            let t = rng.next() * 0.5 - 0.1;
            let at = Point::new((rng.next() - 0.5) * 2.0 * r, (rng.next() - 0.5) * 2.0 * r);
            let v = ripple_lighten(t, at, r);
            assert!((0.0..=RIPPLE_STRENGTH).contains(&v));
            if !(0.0..RIPPLE_DURATION).contains(&t) {
                assert_eq!(v, 0.0);
            }
            peak = peak.max(v);
        }
        assert!(peak > 0.1, "{peak}");
        // At mid-ripple the centre is dark and a ring around 0.5 R is lit.
        let t = RIPPLE_DURATION / 2.0;
        assert_eq!(ripple_lighten(t, Point::default(), r), 0.0);
        assert!(ripple_lighten(t, Point::new(0.525 * r, 0.0), r) > 0.14);
    }

    #[test]
    fn reveal_grows_from_the_centre() {
        let r = 45.0;
        let centre = Point::default();
        let edge = Point::new(r, 0.0);
        assert_eq!(reveal_opacity(0.0, edge, r), 0.0);
        assert!(reveal_opacity(0.2, centre, r) > reveal_opacity(0.2, edge, r));
        let mut rng = Lcg(5);
        for _ in 0..2000 {
            let at = Point::new((rng.next() - 0.5) * 2.0 * r, (rng.next() - 0.5) * 2.0 * r);
            let t = rng.next() * 0.6;
            assert!((0.0..=1.0).contains(&reveal_opacity(t, at, r)));
            assert_eq!(reveal_opacity(REVEAL_DURATION, at, r), 1.0);
            assert_eq!(reveal_opacity(5.0, at, r), 1.0);
        }
        // Monotonic in time.
        let mut last = 0.0;
        for i in 0..=60 {
            let o = reveal_opacity(f64::from(i) * 0.01, Point::new(20.0, 0.0), r);
            assert!(o >= last);
            last = o;
        }
    }

    #[test]
    fn crossfade_is_forty_milliseconds() {
        assert_eq!(crossfade_alpha(-1.0), 0.0);
        assert_eq!(crossfade_alpha(0.0), 0.0);
        assert!((crossfade_alpha(0.02) - 0.5).abs() < 1e-12);
        assert_eq!(crossfade_alpha(0.04), 1.0);
        assert_eq!(crossfade_alpha(1.0), 1.0);
    }

    #[test]
    fn particles_stay_in_their_sector_and_band() {
        let band = ParticleBand {
            inner: 38.25,
            outer: 42.75,
        };
        let width = crate::layout::particle_sector_width();
        let mut rng = Lcg(21);
        for _ in 0..5000 {
            let seed = (rng.next() * 1e9) as u64;
            let k = (rng.next() * 6.0) as usize;
            let s = (rng.next() * 16.0) as u8;
            let to = crate::layout::particle_angle(k, s);
            let t = rng.next() * 300.0;
            let (a, r) = particle_pose(seed, t, k, to, to, 1.0, &band);
            assert!(
                (a - to).abs() < PARTICLE_WOBBLE,
                "wobble {}",
                (a - to).abs()
            );
            assert!(PARTICLE_WOBBLE < width / 3.0);
            assert!(r - dims::PARTICLE_R >= band.inner - 1e-9);
            assert!(r + dims::PARTICLE_R <= band.outer + 1e-9);
            assert_eq!((a, r), particle_pose(seed, t, k, to, to, 1.0, &band));
        }
    }

    #[test]
    fn particles_glide_between_sector_centres() {
        let band = ParticleBand {
            inner: 38.25,
            outer: 42.75,
        };
        let from = crate::layout::particle_angle(2, 3);
        let to = crate::layout::particle_angle(2, 9);
        let (a0, _) = particle_pose(1, 4.0, 2, from, to, 0.0, &band);
        let (a1, _) = particle_pose(1, 4.0, 2, from, to, PARTICLE_GLIDE / 2.0, &band);
        let (a2, _) = particle_pose(1, 4.0, 2, from, to, PARTICLE_GLIDE, &band);
        assert!((a0 - from).abs() < PARTICLE_WOBBLE);
        assert!(a1 > from + 0.2 * (to - from) && a1 < to - 0.2 * (to - from));
        assert!((a2 - to).abs() < PARTICLE_WOBBLE);
    }

    #[test]
    fn session_seed_is_stable_and_spread() {
        assert_eq!(seed_from_session(7), seed_from_session(7));
        assert_ne!(seed_from_session(7), seed_from_session(8));
    }
}
