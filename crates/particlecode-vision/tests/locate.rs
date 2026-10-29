//! `locate` on synthetic renders: profile, orientation, accuracy and false positives.
#![allow(clippy::print_stderr)] // timing diagnostics

mod common;

use common::{
    Degrade, Rng, anchors_error_pitch, busy_image, homography_error_pitch, paste, render, scene,
};
use particlecode_core::Profile;
use particlecode_vision::{Lock, locate};

struct Found {
    lock: Lock,
    anchors_err: f64,
    field_err: f64,
}

fn find(profile: Profile, d: &Degrade, seed: u64) -> Option<Found> {
    let sc = scene(profile, seed);
    let img = render(&sc, d);
    let yuv = img.yuv();
    let lock = locate(&yuv.y)?;
    Some(Found {
        anchors_err: anchors_error_pitch(&lock, &img.truth),
        field_err: homography_error_pitch(&lock.homography, &img.truth, profile),
        lock,
    })
}

const PROFILES: [(Profile, f64); 3] = [(Profile::S, 10.0), (Profile::M, 7.0), (Profile::L, 6.0)];

#[test]
fn locates_clean_frames_at_all_profiles() {
    for (profile, ppp) in PROFILES {
        let f = find(
            profile,
            &Degrade {
                ppp,
                ..Degrade::default()
            },
            3,
        )
        .expect("lock");
        assert_eq!(f.lock.profile, profile);
        assert!(
            f.field_err < 0.1,
            "{profile:?}: field error {} p",
            f.field_err
        );
        assert!(
            (f.lock.pitch_px / ppp - 1.0).abs() < 0.01,
            "{profile:?}: pitch {}",
            f.lock.pitch_px
        );
        assert!(
            f.lock.center_px.is_some(),
            "{profile:?}: the center anchor is found on a clean frame"
        );
    }
}

#[test]
fn locates_all_rotations() {
    for (profile, ppp) in PROFILES {
        for rot in [90.0, 180.0, 270.0, 33.0, 211.0] {
            let d = Degrade {
                ppp,
                rot_deg: rot,
                ..Degrade::default()
            };
            let f =
                find(profile, &d, 5).unwrap_or_else(|| panic!("{profile:?} rot {rot}: no lock"));
            assert_eq!(f.lock.profile, profile, "rot {rot}");
            assert!(f.field_err < 0.1, "{profile:?} rot {rot}: {}", f.field_err);
        }
    }
}

#[test]
fn locates_degraded_frames() {
    for (profile, ppp) in PROFILES {
        let d = Degrade {
            ppp,
            blur: 1.0,
            noise: 4.0,
            tilt_deg: 15.0,
            gain: 0.8,
            ..Degrade::default()
        };
        let f = find(profile, &d, 7).unwrap_or_else(|| panic!("{profile:?}: no lock"));
        assert_eq!(f.lock.profile, profile);
        // The anchors are exact whatever happens; the field is exact only with the center
        // mark (perspective), and good to a couple of pitches without it — `read` refines it.
        assert!(
            f.anchors_err < 0.15,
            "{profile:?}: anchors off by {} p",
            f.anchors_err
        );
        let limit = if f.lock.center_px.is_some() { 0.3 } else { 3.0 };
        assert!(
            f.field_err < limit,
            "{profile:?}: field error {} p",
            f.field_err
        );
    }
}

#[test]
fn locks_random_poses() {
    let mut rng = Rng::new(99);
    let mut tried = 0;
    for (profile, ppp) in PROFILES {
        for k in 0..6 {
            let d = Degrade {
                ppp: ppp * (0.9 + 0.3 * rng.f64()),
                blur: 0.4 + 0.8 * rng.f64(),
                noise: 8.0 * rng.f64(),
                tilt_deg: 25.0 * rng.f64(),
                rot_deg: 360.0 * rng.f64(),
                gain: 0.6 + 0.5 * rng.f64(),
                ..Degrade::default()
            };
            let f = find(profile, &d, 100 + k)
                .unwrap_or_else(|| panic!("{profile:?} pose {d:?}: no lock"));
            assert_eq!(f.lock.profile, profile, "{d:?}");
            assert!(
                f.anchors_err < 0.2,
                "{profile:?} {d:?}: anchors off by {}",
                f.anchors_err
            );
            tried += 1;
        }
    }
    assert_eq!(tried, 18);
}

#[test]
fn locates_a_code_inside_a_bigger_picture() {
    let sc = scene(Profile::M, 4);
    let d = Degrade {
        ppp: 6.0,
        blur: 1.0,
        noise: 3.0,
        tilt_deg: 10.0,
        ..Degrade::default()
    };
    let img = render(&sc, &d);
    // 1080p frame with the code off-center on a dark table.
    let big = paste(&img, 1920, 1080, [50, 48, 46], 1100, 150);
    let yuv = big.yuv();
    let lock = locate(&yuv.y).expect("lock in 1080p frame");
    assert_eq!(lock.profile, Profile::M);
    assert!(anchors_error_pitch(&lock, &big.truth) < 0.15);
}

#[test]
fn busy_images_have_no_lock() {
    for seed in 0..8 {
        let img = busy_image(640, 480, seed, seed % 2 == 0);
        let yuv = img.yuv();
        assert!(locate(&yuv.y).is_none(), "false lock on busy image {seed}");
    }
}

#[test]
fn blank_and_tiny_images_have_no_lock() {
    let flat = particlecode_vision::Plane::filled(300, 300, 200);
    assert!(locate(&flat).is_none());
    let tiny = particlecode_vision::Plane::filled(10, 10, 0);
    assert!(locate(&tiny).is_none());
    let empty = particlecode_vision::Plane::new(0, 0, vec![]);
    assert!(locate(&empty).is_none());
    // A buffer shorter than its claimed size.
    let short = particlecode_vision::Plane::new(500, 500, vec![0; 1000]);
    assert!(locate(&short).is_none());
}

#[test]
fn locate_is_fast_on_1080p() {
    let sc = scene(Profile::M, 4);
    let d = Degrade {
        ppp: 8.0,
        blur: 1.0,
        noise: 3.0,
        ..Degrade::default()
    };
    let img = render(&sc, &d);
    let big = paste(&img, 1920, 1080, [50, 48, 46], 600, 100);
    let yuv = big.yuv420();
    let t = std::time::Instant::now();
    let lock = locate(&yuv.y);
    let ms = t.elapsed().as_secs_f64() * 1e3;
    eprintln!("locate on 1080p (test profile, opt-level 1): {ms:.0} ms");
    assert!(lock.is_some());
    // Generous: release builds take a few ms; this guards against accidental quadratic work.
    assert!(ms < 2000.0, "{ms} ms");
}
