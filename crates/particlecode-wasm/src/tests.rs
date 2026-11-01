//! Native tests: the frame schedule, byte equality with the painter, and the round trip
//! through the phone's decoder.

use particlecode_core::motion::{MotionConfig, seed_from_session};
use particlecode_core::session::{DisplayInfo, Session, SessionConfig};
use particlecode_ffi::api::decoder::ParticleDecoder;
use particlecode_ffi::api::dto::ProgressDto;
use particlecode_render::{RenderConfig, render, render_animated};

use super::*;

const ORIGIN: f64 = 1_760_000_000.0;
const ID: u16 = 0xBEEF;
const KEY: [u8; 32] = [7; 32];

/// The pairing JSON the console draws: 253 bytes of UTF-8.
fn pairing_json() -> Vec<u8> {
    let head = r#"{"v":1,"k":"BKxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx","t":1760000000,"l":"Chrome on macOS","s":""#;
    let tail = r#""}"#;
    let pad = 253 - head.len() - tail.len();
    let json = format!("{head}{}{tail}", "y".repeat(pad));
    assert_eq!(json.len(), 253);
    json.into_bytes()
}

fn canvas(payload: &[u8], profile: &str, motion: bool) -> ParticleCanvas {
    ParticleCanvas::with_identity(payload, profile, 10, ORIGIN, motion, ID, KEY).unwrap()
}

/// The session `canvas` builds, for computing what the painter should have drawn.
fn session(payload: &[u8], profile: Profile) -> Session {
    let display = DisplayInfo {
        width_px: 1080,
        height_px: 1080,
        width_mm: 100.0,
        height_mm: 100.0,
    };
    let config = SessionConfig {
        profile: Some(profile),
        fps: 10,
        ..SessionConfig::default()
    };
    Session::with_identity(payload, display, &config, ID, KEY, ORIGIN as u64).unwrap()
}

fn render_config(size_px: u32) -> RenderConfig {
    RenderConfig {
        size_px,
        fps: 10,
        ..RenderConfig::default()
    }
}

/// A wall clock 37 s after the origin, so a test that passes proves the clock argument is
/// used and not the origin. All the offsets used here stay inside one second.
fn wall_ms(elapsed_ms: f64) -> f64 {
    (ORIGIN + 37.0) * 1000.0 + elapsed_ms
}

fn wall_unix() -> u64 {
    ORIGIN as u64 + 37
}

#[test]
fn data_index_follows_elapsed_time_not_ticks() {
    assert_eq!(data_index(0.0, 10), 0);
    assert_eq!(data_index(99.9, 10), 0);
    assert_eq!(data_index(100.0, 10), 1);
    assert_eq!(data_index(250.0, 10), 2);
    assert_eq!(data_index(1000.0, 10), 10);
    assert_eq!(data_index(1000.0, 24), 24);
    assert_eq!(data_index(-5.0, 10), 0);
    assert_eq!(data_index(f64::NAN, 10), 0);
    assert_eq!(data_index(f64::INFINITY, 10), 0);
    assert_eq!(data_index(1e30, 10), u32::MAX);
}

#[test]
fn unix_seconds_floors_and_guards() {
    assert_eq!(unix_seconds(1_999.0), 1);
    assert_eq!(unix_seconds(2_000.0), 2);
    assert_eq!(unix_seconds(-1.0), 0);
    assert_eq!(unix_seconds(f64::NAN), 0);
}

#[test]
fn many_paints_inside_one_frame_keep_the_index() {
    let mut c = canvas(b"123456", "s", true);
    // A 120 Hz display ticks about every 8.3 ms: twelve paints per 100 ms data frame.
    for k in 0..12 {
        assert!(c.tick(f64::from(k) * 8.3, wall_ms(0.0), 120));
        assert_eq!(c.frame_index(), 0);
    }
    assert!(c.tick(100.0, wall_ms(100.0), 120));
    assert_eq!(c.frame_index(), 1);
    // The clock standing still (a paused loop) repaints nothing.
    assert!(!c.tick(100.0, wall_ms(100.0), 120));
}

#[test]
fn static_code_repaints_only_when_the_frame_or_size_changes() {
    let mut c = canvas(b"123456", "s", false);
    assert!(c.tick(0.0, wall_ms(0.0), 120));
    assert!(
        !c.tick(40.0, wall_ms(40.0), 120),
        "same data frame, nothing moves"
    );
    assert!(!c.tick(99.0, wall_ms(99.0), 120));
    assert!(c.tick(100.0, wall_ms(100.0), 120), "next data frame");
    assert!(c.tick(110.0, wall_ms(110.0), 90), "new size");
    assert_eq!(c.size_px(), 90);
    assert_eq!(c.pixels_len(), 90 * 90 * 4);
    assert!(!c.tick(120.0, wall_ms(120.0), 90));
}

#[test]
fn animated_tick_is_byte_equal_to_render_animated() {
    let payload = pairing_json();
    let reference = session(&payload, Profile::S);
    let seed = seed_from_session(ID);
    let cfg = render_config(200);
    let mut c = canvas(&payload, "s", true);

    // Frame 0, then 1 with the cross-fade from 0 in flight at +30 ms, the same frame again
    // after the fade, then frame 2.
    for (elapsed, index, prev) in [
        (0.0, 0, None),
        (130.0, 1, Some(0)),
        (180.0, 1, Some(0)),
        (205.0, 2, Some(1)),
    ] {
        assert!(c.tick(elapsed, wall_ms(elapsed), 200));
        let frame = reference.frame_at(index, wall_unix());
        let previous = prev.map(|p| reference.frame_at(p, wall_unix()));
        let t = elapsed / 1000.0;
        let since = if previous.is_some() {
            t - f64::from(index) / 10.0
        } else {
            f64::INFINITY
        };
        let expected = render_animated(
            &frame,
            previous.as_ref(),
            &cfg,
            &MotionConfig::default(),
            seed,
            t,
            since,
        );
        assert_eq!(c.pixels(), expected.data(), "elapsed {elapsed} ms");
    }
}

#[test]
fn static_tick_is_byte_equal_to_render() {
    let reference = session(b"123456", Profile::M);
    let mut c = canvas(b"123456", "m", false);
    assert!(c.tick(0.0, wall_ms(0.0), 160));
    let frame = reference.frame_at(0, wall_unix());
    assert_eq!(c.pixels(), render(&frame, &render_config(160)).data());
}

#[test]
fn the_liveness_second_comes_from_the_wall_clock_argument() {
    let reference = session(b"123456", Profile::S);
    // Any second works for the comparison; they only have to be the ones passed in.
    for unix in [wall_unix(), wall_unix() + 1, wall_unix() + 9] {
        let mut c = canvas(b"123456", "s", false);
        assert!(c.tick(0.0, unix as f64 * 1000.0 + 500.0, 160));
        let frame = reference.frame_at(0, unix);
        assert_eq!(c.pixels(), render(&frame, &render_config(160)).data());
    }
}

#[test]
fn the_buffer_is_opaque_rgba_of_the_requested_size() {
    for motion in [true, false] {
        let mut c = canvas(b"123456", "l", motion);
        assert_eq!(c.pixels_len(), 0);
        assert!(c.tick(0.0, wall_ms(0.0), 64));
        assert_eq!(c.pixels_len(), 64 * 64 * 4);
        assert_eq!(c.pixels().len(), c.pixels_len());
        assert!(
            c.pixels().chunks_exact(4).all(|p| p[3] == 255),
            "straight == premultiplied"
        );
        assert_ne!(c.pixels_ptr(), 0);
    }
}

#[test]
fn the_buffer_is_reused_between_ticks_of_one_size() {
    let mut c = canvas(b"123456", "s", true);
    assert!(c.tick(0.0, wall_ms(0.0), 120));
    let ptr = c.pixels_ptr();
    for k in 1..20 {
        assert!(c.tick(f64::from(k) * 16.0, wall_ms(0.0), 120));
        assert_eq!(c.pixels_ptr(), ptr, "no reallocation on the hot path");
    }
}

#[test]
fn size_is_clamped() {
    let mut c = canvas(b"123456", "s", false);
    assert!(c.tick(0.0, wall_ms(0.0), 0));
    assert_eq!(c.size_px(), 1);
    assert_eq!(c.pixels_len(), 4);
}

#[test]
fn bad_time_counts_as_zero() {
    let mut c = canvas(b"123456", "s", true);
    assert!(c.tick(f64::NAN, f64::NAN, 64));
    assert_eq!(c.frame_index(), 0);
    c.tick(-1.0, -1.0, 64);
    assert_eq!(c.frame_index(), 0);
}

#[test]
fn reset_restarts_the_schedule() {
    let mut c = canvas(b"123456", "s", true);
    assert!(c.tick(550.0, wall_ms(550.0), 64));
    assert_eq!(c.frame_index(), 5);
    c.reset();
    assert!(c.tick(0.0, wall_ms(600.0), 64));
    assert_eq!(c.frame_index(), 0);
}

#[test]
fn time_running_backwards_drops_the_previous_frame() {
    let mut a = canvas(b"123456", "s", true);
    assert!(a.tick(550.0, wall_ms(550.0), 64));
    assert!(a.tick(120.0, wall_ms(120.0), 64));
    assert_eq!(a.frame_index(), 1);
    // Same pixels as a canvas that was never ahead: no stale `previous` leaks in.
    let mut b = canvas(b"123456", "s", true);
    assert!(b.tick(120.0, wall_ms(120.0), 64));
    assert_eq!(a.pixels(), b.pixels());
}

#[test]
fn constructor_rejects_bad_arguments_without_panicking() {
    let new = |profile: &str, fps: u32, origin: f64| {
        ParticleCanvas::with_identity(b"123456", profile, fps, origin, true, ID, KEY)
    };
    assert!(matches!(new("x", 10, ORIGIN), Err(CanvasError::Profile(_))));
    assert!(matches!(new("", 10, ORIGIN), Err(CanvasError::Profile(_))));
    assert!(matches!(new("s", 0, ORIGIN), Err(CanvasError::Fps(0))));
    assert!(matches!(new("s", 241, ORIGIN), Err(CanvasError::Fps(241))));
    assert!(matches!(new("s", 70_000, ORIGIN), Err(CanvasError::Fps(_))));
    assert!(matches!(
        new("s", 10, f64::NAN),
        Err(CanvasError::ClockOrigin(_))
    ));
    assert!(matches!(
        new("s", 10, -1.0),
        Err(CanvasError::ClockOrigin(_))
    ));
    assert!(
        new("S", 240, 0.0).is_ok(),
        "profile is case-insensitive, 240 fps is the limit"
    );
    // An empty payload is the core's call; either way it is a value, not a panic.
    let _ = ParticleCanvas::with_identity(b"", "s", 10, ORIGIN, true, ID, KEY);
}

#[test]
fn random_identity_constructor_works_natively() {
    let c = ParticleCanvas::try_new(b"123456", "s", 10, ORIGIN, true).unwrap();
    assert_eq!(c.fps(), 10);
}

#[test]
fn the_identity_is_exposed() {
    let c = canvas(b"123456", "s", true);
    assert_eq!(c.session_id(), ID);
    assert_eq!(c.fps(), 10);
}

/// Feed successive ticks (a 10 fps animation sampled at 30 Hz, so cross-fades and drift are in
/// the pictures) to the phone's decoder until it reports the payload. Returns the ticks used.
fn round_trip(payload: &[u8], profile: &str, size_px: u32) -> usize {
    let mut c = canvas(payload, profile, true);
    let mut decoder = ParticleDecoder::new();
    for k in 0..300u32 {
        let elapsed = f64::from(k) * 1000.0 / 30.0;
        assert!(c.tick(elapsed, wall_ms(elapsed), size_px));
        let result = decoder.feed_rgba(size_px, size_px, c.pixels_copy());
        if let ProgressDto::Done { payload: got } = result.progress {
            assert_eq!(got, payload);
            return k as usize + 1;
        }
    }
    panic!("the decoder never completed the payload");
}

#[test]
fn round_trip_six_digit_code() {
    let ticks = round_trip(b"482915", "s", 480);
    assert!(ticks < 60, "took {ticks} ticks");
}

#[test]
fn round_trip_pairing_json() {
    let ticks = round_trip(&pairing_json(), "s", 656);
    assert!(ticks < 150, "took {ticks} ticks");
}

#[test]
fn round_trip_other_profiles() {
    round_trip(b"482915", "m", 560);
    round_trip(b"482915", "l", 640);
}

#[test]
fn round_trip_static_code() {
    let payload = pairing_json();
    let mut c = canvas(&payload, "s", false);
    let mut decoder = ParticleDecoder::new();
    for k in 0..100u32 {
        let elapsed = f64::from(k) * 100.0;
        c.tick(elapsed, wall_ms(elapsed), 656);
        let result = decoder.feed_rgba(656, 656, c.pixels_copy());
        if let ProgressDto::Done { payload: got } = result.progress {
            assert_eq!(got, payload);
            return;
        }
    }
    panic!("the decoder never completed the payload");
}
