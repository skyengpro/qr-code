//! The whole pipeline across crates: `Session` frames → `particlecode-render` (animated,
//! breathing wave) → synthetic camera (`particlecode_render::synth`) → `locate` / `read` →
//! `Receiver` → the original payload.

use particlecode_core::motion::{Mood, MotionConfig, seed_from_session};
use particlecode_core::shapes;
use particlecode_core::{
    DisplayInfo, Profile, Progress, Receiver, Session, SessionConfig, ShapeChoice,
};
use particlecode_render::synth::{CameraPose, Degradation, RgbImage, apply_all};
use particlecode_render::{Anim, RenderConfig, render_anim};
use particlecode_vision::{Liveness, YuvFrame, locate, read};

const DISPLAY: DisplayInfo = DisplayInfo {
    width_px: 1080,
    height_px: 2400,
    width_mm: 68.0,
    height_mm: 151.0,
};

fn payload(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 31 % 251) as u8 ^ 0x5A).collect()
}

/// Render frame `i` mid-way through its 100 ms slot and photograph it.
fn camera_shot(session: &Session, i: u32, size_px: u32, motion: MotionConfig) -> YuvFrame {
    let frame = session.frame(i);
    let cfg = RenderConfig {
        size_px,
        ..RenderConfig::default()
    };
    let seed = seed_from_session(session.session_id());
    let anim = Anim::at(motion, seed, f64::from(i) * 0.1 + 0.06);
    let img = RgbImage::from_pixmap(&render_anim(&frame, &cfg, &anim));
    let shot = apply_all(
        &img,
        &[
            Degradation::Tilt {
                pose: CameraPose {
                    rotate_deg: 8.0 + f64::from(i % 5) * 7.0,
                    tilt_x_deg: 12.0,
                    tilt_y_deg: -8.0,
                    scale: 0.85,
                },
                out_w: size_px * 5 / 4,
                out_h: size_px,
                background: particlecode_core::profile::Rgb(40, 38, 36),
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
            Degradation::Blur { sigma: 0.9 },
            Degradation::Noise {
                sigma: 4.0,
                seed: u64::from(i),
            },
        ],
    );
    YuvFrame::from_rgb(shot.width as usize, shot.height as usize, &shot.data)
}

fn transfer(config: &SessionConfig, data: &[u8], size_px: u32, drop_every: u32) -> u32 {
    let session = Session::with_identity(data, DISPLAY, config, 0xC0DE, [3; 32], 1_700_000_000)
        .expect("session");
    let mut rx = Receiver::new();
    let motion = MotionConfig {
        mood: Mood::Breathing,
        ..MotionConfig::default()
    };
    for i in 0..session.frames_per_pass() * 4 {
        if drop_every > 0 && i % drop_every == drop_every - 1 {
            continue;
        }
        let yuv = camera_shot(&session, i, size_px, motion);
        let lock = locate(&yuv.y).unwrap_or_else(|| panic!("frame {i}: no lock"));
        let (header, body, liveness) = read(&yuv, &lock, &|p, s| rx.dot_order(p, s)).expect("read");
        assert_eq!(header.packet_seed, session.frame(i).header.packet_seed);
        assert!(
            matches!(
                liveness,
                Liveness::Read {
                    consistent: true,
                    ..
                }
            ),
            "{liveness:?}"
        );
        match rx.feed_decoded(&header, &body) {
            Progress::Done(got) => {
                assert_eq!(got, data);
                return i + 1;
            }
            Progress::Rejected(e) => panic!("frame {i} rejected: {e}"),
            Progress::Accepted { .. } => {}
        }
    }
    panic!("transfer did not complete");
}

#[test]
fn six_kilobytes_through_the_camera_at_profile_m() {
    let config = SessionConfig::default();
    let frames = transfer(&config, &payload(6000), 760, 3);
    // 5 source symbols at profile M; every third frame is dropped.
    assert!(frames <= 9, "took {frames} frames");
}

#[test]
fn letter_shape_and_profile_s() {
    let config = SessionConfig {
        profile: Some(Profile::S),
        shape: ShapeChoice::Require(shapes::letter('P').expect("P")),
        ..SessionConfig::default()
    };
    transfer(&config, &payload(1500), 560, 0);
}

#[test]
fn static_print_frame() {
    let config = SessionConfig {
        is_static: true,
        ..SessionConfig::default()
    };
    assert_eq!(transfer(&config, &payload(900), 760, 0), 1);
}
