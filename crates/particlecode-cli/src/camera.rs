//! A synthetic camera in front of the encoder: render with motion, degrade like a phone
//! photo, decode with the vision crate. Used by `roundtrip --camera` and the
//! `camera_roundtrip` example.

use particlecode_core::motion::{Mood, MotionConfig, seed_from_session};
use particlecode_core::profile::{Profile, Rgb};
use particlecode_core::{Liveness, Session};
use particlecode_render::synth::{CameraPose, Degradation, RgbImage, apply_all};
use particlecode_render::{Anim, RenderConfig, render_anim};
use particlecode_vision::YuvFrame;

use crate::decode::{Decoder, FrameReport, Outcome};

/// How the synthetic camera sees the screen.
#[derive(Debug, Clone, PartialEq)]
pub struct Camera {
    /// Gaussian blur sigma in pixels.
    pub blur: f64,
    /// Sensor noise sigma in 0..255 units.
    pub noise: f64,
    /// Tilt of the screen plane in degrees (about the horizontal axis; the vertical tilt is
    /// two thirds of it, the opposite way).
    pub tilt: f64,
}

impl Default for Camera {
    /// A mild preset: 0.9 px blur, noise 4, tilt 12 degrees.
    fn default() -> Self {
        Camera {
            blur: 0.9,
            noise: 4.0,
            tilt: 12.0,
        }
    }
}

/// Side of the rendered screen image, large enough that dots stay resolvable after the
/// tilt and downscale.
pub fn screen_px(profile: Profile) -> u32 {
    match profile {
        Profile::S => 560,
        Profile::M => 760,
        Profile::L => 1000,
    }
}

impl Camera {
    /// Photograph frame `i` of `session`: breathing motion sampled mid-way through the
    /// frame's 100 ms slot, a rotation that varies from frame to frame, a warm colour cast
    /// and slightly low exposure, then blur and noise.
    pub fn shoot(&self, session: &Session, i: u32) -> YuvFrame {
        let size_px = screen_px(session.profile());
        let cfg = RenderConfig {
            size_px,
            ..RenderConfig::default()
        };
        let motion = MotionConfig {
            mood: Mood::Breathing,
            ..MotionConfig::default()
        };
        let seed = seed_from_session(session.session_id());
        let anim = Anim::at(motion, seed, f64::from(i) * 0.1 + 0.06);
        let screen = RgbImage::from_pixmap(&render_anim(&session.frame(i), &cfg, &anim));
        let mut chain = vec![
            Degradation::Tilt {
                pose: CameraPose {
                    rotate_deg: 8.0 + f64::from(i % 5) * 7.0,
                    tilt_x_deg: self.tilt,
                    tilt_y_deg: -self.tilt * 2.0 / 3.0,
                    scale: 0.85,
                },
                out_w: size_px * 5 / 4,
                out_h: size_px,
                background: Rgb(40, 38, 36),
            },
            Degradation::ColorCast {
                r: 1.06,
                g: 1.0,
                b: 0.9,
            },
            Degradation::Exposure {
                gain: 0.85,
                offset: 4.0,
            },
        ];
        if self.blur > 0.0 {
            chain.push(Degradation::Blur { sigma: self.blur });
        }
        if self.noise > 0.0 {
            chain.push(Degradation::Noise {
                sigma: self.noise,
                seed: u64::from(i),
            });
        }
        let shot = apply_all(&screen, &chain);
        YuvFrame::from_rgb(shot.width as usize, shot.height as usize, &shot.data)
    }
}

/// What happened to one displayed frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The channel lost the frame (never photographed).
    Dropped,
    /// The frame was photographed and decoded.
    Frame(FrameReport),
}

/// One displayed frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// Frame index in the session.
    pub index: u32,
    /// What came of it.
    pub event: Event,
}

/// Summary of a camera round trip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Frames shown (dropped ones included).
    pub frames_shown: u32,
    /// Frames never photographed.
    pub frames_dropped: u32,
    /// Photographed frames where the anchors were not found.
    pub lock_failures: u32,
    /// Frames locked but unreadable (cut off, bad calibration).
    pub read_failures: u32,
    /// Frames the receiver rejected (Reed-Solomon could not correct them, ...).
    pub rs_rejections: u32,
    /// Frames the receiver used.
    pub frames_accepted: u32,
    /// Frames whose particles were read and matched the packet seed.
    pub liveness_consistent: u32,
    /// Frames needed with a perfect channel: `Σ K_s`.
    pub frames_minimum: u32,
    /// The payload came back and equals the original.
    pub success: bool,
}

/// Show frames `0..max_frames` of `session` (skipping every `drop_every`-th, 0 for none)
/// to `camera`, decode what it sees and stop when the payload is complete. `on_step` is
/// called after every frame.
pub fn run(
    session: &Session,
    payload: &[u8],
    camera: &Camera,
    drop_every: u32,
    max_frames: u32,
    mut on_step: impl FnMut(&Step, &Decoder),
) -> Report {
    let packets = session.packets();
    let mut report = Report {
        frames_shown: 0,
        frames_dropped: 0,
        lock_failures: 0,
        read_failures: 0,
        rs_rejections: 0,
        frames_accepted: 0,
        liveness_consistent: 0,
        frames_minimum: (0..packets.segment_count())
            .filter_map(|s| packets.source_symbols(s))
            .sum(),
        success: false,
    };
    let mut decoder = Decoder::new();
    for index in 0..max_frames {
        report.frames_shown += 1;
        if drop_every > 0 && (index + 1) % drop_every == 0 {
            report.frames_dropped += 1;
            on_step(
                &Step {
                    index,
                    event: Event::Dropped,
                },
                &decoder,
            );
            continue;
        }
        let fr = decoder.feed_yuv(&camera.shoot(session, index));
        match &fr.outcome {
            Outcome::NoLock => report.lock_failures += 1,
            Outcome::Unreadable(_) => report.read_failures += 1,
            Outcome::Rejected(_) => report.rs_rejections += 1,
            Outcome::Accepted { .. } | Outcome::Done => report.frames_accepted += 1,
        }
        if matches!(
            fr.liveness,
            Liveness::Read {
                consistent: true,
                ..
            }
        ) {
            report.liveness_consistent += 1;
        }
        let done = fr.outcome == Outcome::Done;
        on_step(
            &Step {
                index,
                event: Event::Frame(fr),
            },
            &decoder,
        );
        if done {
            report.success = decoder.payload() == Some(payload);
            break;
        }
    }
    report
}

/// One-line description of a step for terminal output.
pub fn describe(step: &Step, decoder: &Decoder) -> String {
    match &step.event {
        Event::Dropped => format!("frame {:>3}: dropped (not photographed)", step.index),
        Event::Frame(fr) => {
            crate::decode::describe(&format!("frame {:>3}", step.index), fr, decoder)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::SessionSpec;

    fn session(data: &[u8], profile: Profile) -> Session {
        SessionSpec {
            profile,
            session_id: Some(0xC0DE),
            key: Some([3; 32]),
            clock_origin: Some(1_700_000_000),
            ..SessionSpec::default()
        }
        .build(data)
        .expect("session")
    }

    #[test]
    fn small_payload_survives_the_camera() {
        let data: Vec<u8> = (0..700).map(|i| (i * 13 % 251) as u8).collect();
        let s = session(&data, Profile::S);
        let mut steps = 0;
        let r = run(&s, &data, &Camera::default(), 0, 12, |_, _| steps += 1);
        assert!(r.success, "{r:?}");
        assert_eq!(steps, r.frames_shown);
        assert_eq!(
            r.lock_failures + r.read_failures + r.rs_rejections,
            0,
            "{r:?}"
        );
        assert!(r.liveness_consistent > 0);
    }

    #[test]
    fn dropped_frames_are_never_photographed() {
        let data = vec![5u8; 300];
        let s = session(&data, Profile::S);
        let r = run(&s, &data, &Camera::default(), 1, 4, |st, _| {
            assert_eq!(st.event, Event::Dropped);
        });
        assert!(!r.success);
        assert_eq!((r.frames_shown, r.frames_dropped), (4, 4));
    }

    #[test]
    fn hopeless_camera_reports_failures_without_panicking() {
        let data = vec![5u8; 300];
        let s = session(&data, Profile::S);
        let cam = Camera {
            blur: 12.0,
            noise: 60.0,
            tilt: 75.0,
        };
        let r = run(&s, &data, &cam, 0, 3, |_, _| {});
        assert!(!r.success);
        assert_eq!(
            r.lock_failures + r.read_failures + r.rs_rejections + r.frames_accepted,
            3
        );
    }
}
