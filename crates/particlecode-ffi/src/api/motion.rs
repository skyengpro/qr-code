//! Animation curves, as thin wrappers over `particlecode_core::motion`.
//!
//! A Dart or JS painter can reimplement these (they are small, deterministic and specified
//! in the core) or call through. Per-dot calls cost an FFI crossing each, so use the
//! `*_batch` variants to animate a whole frame at once.
//!
//! Times are seconds, lengths are pitch units, angles are radians. `half_side` is
//! [`FrameDto::frame_half_side`](super::dto::FrameDto::frame_half_side). Nothing here is
//! accumulated: pass the painter's clock, so a dropped render frame never drifts the field.

use flutter_rust_bridge::frb;

use particlecode_core::layout::{ParticleBand, Point};
use particlecode_core::motion::{self, Mood, MotionConfig};
use serde::{Deserialize, Serialize};

/// How dots drift around their lattice points. Mirrors [`Mood`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MoodDto {
    /// A slow sine wave travelling across the field (the default).
    #[default]
    Breathing,
    /// A smooth seeded noise field: wind through grass.
    FlowField,
}

/// Which animation layers a painter applies. Mirrors [`MotionConfig`]; the default is the
/// spec's default (breathing wave, nothing else).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)] // independent on/off layers, one flag each
pub struct MotionConfigDto {
    /// Dot drift and radius pulse.
    pub mood: MoodDto,
    /// A ring of brightness on every completed segment.
    pub ripple: bool,
    /// The three hero rings swell in a slow, offset pulse.
    pub anchor_heartbeat: bool,
    /// Particles leave a fading tail.
    pub particle_trails: bool,
    /// Dots fade in from the centre at session start.
    pub reveal: bool,
    /// Hue-rotating cross-fade at a data-frame switch.
    pub shimmer: bool,
}

impl From<MotionConfigDto> for MotionConfig {
    fn from(c: MotionConfigDto) -> Self {
        MotionConfig {
            mood: match c.mood {
                MoodDto::Breathing => Mood::Breathing,
                MoodDto::FlowField => Mood::FlowField,
            },
            ripple: c.ripple,
            anchor_heartbeat: c.anchor_heartbeat,
            particle_trails: c.particle_trails,
            reveal: c.reveal,
            shimmer: c.shimmer,
        }
    }
}

/// Every optional layer on, breathing mood.
#[frb(sync)]
pub fn motion_config_all() -> MotionConfigDto {
    MotionConfigDto {
        mood: MoodDto::Breathing,
        ripple: true,
        anchor_heartbeat: true,
        particle_trails: true,
        reveal: true,
        shimmer: true,
    }
}

/// A 2-D vector or point, pitch units.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PointDto {
    /// Horizontal, rightwards.
    pub x: f64,
    /// Vertical, downwards.
    pub y: f64,
}

/// Pose of a particle: where to draw it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ParticlePoseDto {
    /// Angle around the frame center, radians (+x towards +y).
    pub angle: f64,
    /// Distance from the frame center, pitch units.
    pub radius: f64,
}

/// The timing constants painters need, in seconds or pitch units.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MotionConstants {
    /// Interval of the data-frame timer (10 fps), independent of paint timing.
    pub data_frame_interval: f64,
    /// Cross-fade at a data-frame switch.
    pub crossfade_duration: f64,
    /// Particle glide at a data-frame switch.
    pub particle_glide: f64,
    /// Ripple on segment completion.
    pub ripple_duration: f64,
    /// Reveal at session start.
    pub reveal_duration: f64,
    /// Length of a particle trail, pitch units.
    pub trail_length: f64,
    /// Largest dot drift, pitch units.
    pub max_drift: f64,
}

/// The constants of `particlecode_core::motion` plus the data-frame interval.
#[frb(sync)]
pub fn motion_constants() -> MotionConstants {
    MotionConstants {
        data_frame_interval: 0.1,
        crossfade_duration: motion::CROSSFADE_DURATION,
        particle_glide: motion::PARTICLE_GLIDE,
        ripple_duration: motion::RIPPLE_DURATION,
        reveal_duration: motion::REVEAL_DURATION,
        trail_length: motion::TRAIL_LENGTH,
        max_drift: motion::MAX_DRIFT,
    }
}

/// The motion seed every painter must use for a session, so the app, the GIF packager and
/// the web painter agree on direction and phase.
#[frb(sync)]
pub fn motion_seed(session_id: u16) -> u64 {
    motion::seed_from_session(session_id)
}

/// Offset of a dot at rest position `(x, y)` from its lattice point; length at most 0.12.
#[frb(sync)]
pub fn dot_offset(
    config: MotionConfigDto,
    seed: u64,
    t: f64,
    x: f64,
    y: f64,
    half_side: f64,
) -> PointDto {
    let o = motion::dot_offset(&config.into(), seed, t, Point::new(x, y), half_side);
    PointDto { x: o.x, y: o.y }
}

/// Offsets for many dots at once: `xs`/`ys` are rest positions, the result is interleaved
/// `[dx0, dy0, dx1, dy1, ...]`. The shorter of `xs` and `ys` decides the count.
#[frb(sync)]
pub fn dot_offsets_batch(
    config: MotionConfigDto,
    seed: u64,
    t: f64,
    xs: Vec<f64>,
    ys: Vec<f64>,
    half_side: f64,
) -> Vec<f64> {
    let cfg = config.into();
    xs.iter()
        .zip(&ys)
        .flat_map(|(&x, &y)| {
            let o = motion::dot_offset(&cfg, seed, t, Point::new(x, y), half_side);
            [o.x, o.y]
        })
        .collect()
}

/// Multiplier for a dot's radius at `(x, y)`, in `[0.95, 1.05]`.
#[frb(sync)]
pub fn dot_radius_scale(
    config: MotionConfigDto,
    seed: u64,
    t: f64,
    x: f64,
    y: f64,
    half_side: f64,
) -> f64 {
    motion::dot_radius_scale(&config.into(), seed, t, Point::new(x, y), half_side)
}

/// [`dot_radius_scale`] for many dots; one scale per `(xs[i], ys[i])`.
#[frb(sync)]
pub fn dot_radius_scales_batch(
    config: MotionConfigDto,
    seed: u64,
    t: f64,
    xs: Vec<f64>,
    ys: Vec<f64>,
    half_side: f64,
) -> Vec<f64> {
    let cfg = config.into();
    xs.iter()
        .zip(&ys)
        .map(|(&x, &y)| motion::dot_radius_scale(&cfg, seed, t, Point::new(x, y), half_side))
        .collect()
}

/// Pose of particle `k` (`0..6`). Within 60 ms of a switch the particle glides from
/// `from_angle` to `to_angle`; otherwise it wobbles around `to_angle` inside the band
/// `band_inner..band_outer`.
#[allow(clippy::too_many_arguments)] // mirrors the core function; flat for the bridge
#[frb(sync)]
pub fn particle_pose(
    seed: u64,
    t: f64,
    k: u32,
    from_angle: f64,
    to_angle: f64,
    t_since_switch: f64,
    band_inner: f64,
    band_outer: f64,
) -> ParticlePoseDto {
    let band = ParticleBand {
        inner: band_inner,
        outer: band_outer,
    };
    let (angle, radius) = motion::particle_pose(
        seed,
        t,
        k as usize,
        from_angle,
        to_angle,
        t_since_switch,
        &band,
    );
    ParticlePoseDto { angle, radius }
}

/// Opacity of the new data frame `t_since_switch` seconds after the switch: 0 at the
/// switch, 1 from 40 ms on.
#[frb(sync)]
pub fn crossfade_alpha(t_since_switch: f64) -> f64 {
    motion::crossfade_alpha(t_since_switch)
}

/// Opacity of a dot at `(x, y)`, `t_since_start` seconds into the session. Fully opaque
/// from 600 ms on.
#[frb(sync)]
pub fn reveal_opacity(t_since_start: f64, x: f64, y: f64, half_side: f64) -> f64 {
    motion::reveal_opacity(t_since_start, Point::new(x, y), half_side)
}

/// Scale of anchor `anchor_index` (`0..3` hero, `3` center) at time `t`; 1.0 unless the
/// heartbeat is on, never above 1.10.
#[frb(sync)]
pub fn anchor_scale(config: MotionConfigDto, t: f64, anchor_index: u32) -> f64 {
    motion::anchor_scale(&config.into(), t, anchor_index as usize)
}

/// How much a dot at `(x, y)` lightens (`0..=0.15`, fraction of the way to white)
/// `t_since_segment_complete` seconds after a segment completed.
#[frb(sync)]
pub fn ripple_lighten(t_since_segment_complete: f64, x: f64, y: f64, half_side: f64) -> f64 {
    motion::ripple_lighten(t_since_segment_complete, Point::new(x, y), half_side)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrappers_agree_with_the_core() {
        let cfg = motion_config_all();
        let seed = motion_seed(0x1234);
        assert_eq!(seed, motion::seed_from_session(0x1234));
        let at = Point::new(3.0, -4.5);
        let core = motion::dot_offset(&cfg.into(), seed, 1.3, at, 40.0);
        let got = dot_offset(cfg, seed, 1.3, at.x, at.y, 40.0);
        assert_eq!((got.x, got.y), (core.x, core.y));
        assert!(got.x.hypot(got.y) <= motion::MAX_DRIFT);
        let s = dot_radius_scale(cfg, seed, 1.3, at.x, at.y, 40.0);
        assert!((0.95..=1.05).contains(&s));
    }

    #[test]
    #[allow(clippy::float_cmp)] // the batch must be bit-identical to single calls
    fn batches_match_single_calls() {
        let cfg = MotionConfigDto {
            mood: MoodDto::FlowField,
            ..MotionConfigDto::default()
        };
        let xs = vec![0.0, 1.5, -7.0, 12.25];
        let ys = vec![0.0, -3.0, 9.0, 2.5];
        let offs = dot_offsets_batch(cfg, 5, 0.7, xs.clone(), ys.clone(), 36.0);
        let scales = dot_radius_scales_batch(cfg, 5, 0.7, xs.clone(), ys.clone(), 36.0);
        assert_eq!(offs.len(), 8);
        assert_eq!(scales.len(), 4);
        for i in 0..4 {
            let o = dot_offset(cfg, 5, 0.7, xs[i], ys[i], 36.0);
            assert_eq!((offs[2 * i], offs[2 * i + 1]), (o.x, o.y));
            assert_eq!(scales[i], dot_radius_scale(cfg, 5, 0.7, xs[i], ys[i], 36.0));
        }
        // Mismatched lengths use the shorter list.
        assert_eq!(
            dot_offsets_batch(cfg, 5, 0.0, vec![1.0, 2.0], vec![1.0], 36.0).len(),
            2
        );
    }

    #[test]
    fn curves_have_the_documented_endpoints() {
        assert!(crossfade_alpha(0.0).abs() < f64::EPSILON);
        assert!((crossfade_alpha(0.04) - 1.0).abs() < f64::EPSILON);
        assert!((reveal_opacity(0.6, 10.0, 10.0, 40.0) - 1.0).abs() < f64::EPSILON);
        assert!(reveal_opacity(0.0, 30.0, 0.0, 40.0) < 1e-9);
        assert!(ripple_lighten(-1.0, 0.0, 0.0, 40.0).abs() < f64::EPSILON);
        assert!(ripple_lighten(5.0, 0.0, 0.0, 40.0).abs() < f64::EPSILON);
        // The ring reaches radius 0.5 * 1.05 * R midway through the ripple.
        let mid = ripple_lighten(0.15, 21.0, 0.0, 40.0);
        assert!(mid > 0.0 && mid <= motion::RIPPLE_STRENGTH);
    }

    #[test]
    fn anchors_pulse_only_when_enabled() {
        let off = MotionConfigDto::default();
        let on = motion_config_all();
        for t in [0.0, 0.7, 1.9] {
            assert!((anchor_scale(off, t, 0) - 1.0).abs() < f64::EPSILON);
            assert!((anchor_scale(on, t, 3) - 1.0).abs() < f64::EPSILON);
            let s = anchor_scale(on, t, 1);
            assert!((1.0..=1.10 + 1e-12).contains(&s));
        }
    }

    #[test]
    fn particle_pose_stays_in_the_band() {
        let (inner, outer) = (40.0, 44.0);
        for k in 0..6 {
            for step in 0..50 {
                let p = particle_pose(9, f64::from(step) * 0.37, k, 0.1, 0.2, 1.0, inner, outer);
                assert!(p.radius >= inner && p.radius <= outer);
            }
        }
        let c = motion_constants();
        assert!((c.crossfade_duration - 0.04).abs() < f64::EPSILON);
        assert!((c.data_frame_interval - 0.1).abs() < f64::EPSILON);
    }
}
