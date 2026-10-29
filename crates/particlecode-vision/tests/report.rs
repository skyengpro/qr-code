//! Exploratory measurements, all `#[ignore]`d: they print tables of symbol error rates, lock
//! rates and timings for the synthetic camera. Run with
//! `cargo test -p particlecode-vision --release --test report -- --ignored --nocapture`.
#![allow(clippy::print_stderr)] // the point of this file is to print

mod common;

use common::{Batch, Degrade, batch, paste, render, run_image, scene, symbol_errors};
use particlecode_core::Profile;
use particlecode_vision::{locate, read_with_order};

const PROFILES: [(Profile, f64); 3] = [(Profile::S, 9.0), (Profile::M, 7.0), (Profile::L, 6.0)];

fn print(label: &str, profile: Profile, b: &Batch) {
    let conf: usize = b.outcomes.iter().map(|o| o.confident_errors).sum();
    let unmarked: usize = b.outcomes.iter().map(|o| o.unmarked_errors).sum();
    let erasures: usize = b.outcomes.iter().map(|o| o.erasures).sum();
    let ms = b.outcomes.iter().map(|o| o.ms).sum::<f64>() / b.outcomes.len().max(1) as f64;
    let herr = b.outcomes.iter().map(|o| o.herr).fold(0.0, f64::max);
    eprintln!(
        "{label:<14} {profile:?}: locks {}/{} header errs {} body SER {:6.3}% (wrong {}, confident-wrong {conf}, unmarked {unmarked}, erasures {erasures}) worst H err {herr:.3} p, {ms:.0} ms",
        b.locks,
        b.tried,
        b.header_err,
        100.0 * b.ser(),
        b.body_err
    );
}

#[test]
#[ignore = "exploratory report"]
fn matrix() {
    for (profile, ppp) in PROFILES {
        let base = Degrade {
            ppp,
            ..Degrade::default()
        };
        let spec = Degrade {
            blur: 1.0,
            noise: 4.0,
            tilt_deg: 15.0,
            gain: 0.8,
            ..base.clone()
        };
        let cases: Vec<(&str, Degrade, bool)> = vec![
            ("clean", base.clone(), false),
            ("spec", spec.clone(), false),
            ("spec 4:2:0", spec.clone(), true),
            (
                "spec+wave",
                Degrade {
                    drift: 0.12,
                    ..spec.clone()
                },
                false,
            ),
            (
                "spec+jitter",
                Degrade {
                    jitter: 0.08,
                    ..spec.clone()
                },
                false,
            ),
            (
                "noise12",
                Degrade {
                    blur: 0.8,
                    noise: 12.0,
                    tilt_deg: 10.0,
                    ..base.clone()
                },
                false,
            ),
            (
                "rot90",
                Degrade {
                    rot_deg: 90.0,
                    ..spec.clone()
                },
                false,
            ),
            (
                "rot180",
                Degrade {
                    rot_deg: 180.0,
                    ..spec.clone()
                },
                false,
            ),
            (
                "rot37 tilt25",
                Degrade {
                    rot_deg: 37.0,
                    tilt_deg: 25.0,
                    ..spec.clone()
                },
                false,
            ),
            (
                "dim .5",
                Degrade {
                    gain: 0.5,
                    ..spec.clone()
                },
                false,
            ),
            (
                "warm",
                Degrade {
                    tint: [1.12, 1.0, 0.78],
                    ..spec.clone()
                },
                false,
            ),
            (
                "night light",
                Degrade {
                    tint: [1.0, 0.85, 0.55],
                    ..spec.clone()
                },
                false,
            ),
            (
                "glare .5",
                Degrade {
                    glare: Some((10.0, -5.0, 6.0, 0.5)),
                    ..spec.clone()
                },
                false,
            ),
            (
                "blur 1.5",
                Degrade {
                    blur: 1.5,
                    ..spec.clone()
                },
                false,
            ),
        ];
        for (name, d, c420) in cases {
            print(name, profile, &batch(profile, &d, c420, &[1, 2, 3, 4]));
        }
    }
}

#[test]
#[ignore = "exploratory report"]
fn blur_sweep() {
    for (profile, ppp) in [(Profile::M, 7.0), (Profile::L, 6.0)] {
        for blur in [0.0, 0.5, 1.0, 1.5, 2.0, 2.5] {
            let d = Degrade {
                ppp,
                blur,
                noise: 4.0,
                tilt_deg: 15.0,
                gain: 0.8,
                drift: 0.12,
                ..Degrade::default()
            };
            print(
                &format!("blur {blur}"),
                profile,
                &batch(profile, &d, false, &[1, 2]),
            );
        }
    }
}

#[test]
#[ignore = "exploratory report"]
fn resolution_sweep() {
    for (profile, ppps) in [
        (Profile::S, [6.0, 8.0, 12.0]),
        (Profile::M, [4.5, 5.5, 8.0]),
        (Profile::L, [4.0, 5.0, 7.0]),
    ] {
        for ppp in ppps {
            let d = Degrade {
                ppp,
                blur: 0.12 * ppp,
                noise: 4.0,
                tilt_deg: 15.0,
                gain: 0.8,
                ..Degrade::default()
            };
            print(
                &format!("ppp {ppp}"),
                profile,
                &batch(profile, &d, false, &[1, 2]),
            );
        }
    }
}

#[test]
#[ignore = "timing; use --release"]
fn timing_1080p() {
    for (profile, ppp) in [(Profile::S, 14.0), (Profile::M, 10.0), (Profile::L, 7.5)] {
        let sc = scene(profile, 5);
        let d = Degrade {
            ppp,
            blur: 1.0,
            noise: 3.0,
            tilt_deg: 10.0,
            ..Degrade::default()
        };
        let img = render(&sc, &d);
        let big = paste(
            &img,
            1920,
            1080,
            [60, 62, 66],
            960 - img.width / 2,
            540 - img.height / 2,
        );
        let yuv = big.yuv420();
        let n = 5;
        let (mut t_loc, mut t_read) = (0.0, 0.0);
        let mut errs = 0;
        for _ in 0..n {
            let t = std::time::Instant::now();
            let lock = locate(&yuv.y).expect("lock");
            t_loc += t.elapsed().as_secs_f64();
            let t = std::time::Instant::now();
            let raw = read_with_order(&yuv, &lock, &sc.order).expect("read");
            t_read += t.elapsed().as_secs_f64();
            errs = symbol_errors(&sc.spec.body_symbols(), &raw.body_symbols);
        }
        eprintln!(
            "{profile:?} 1920x1080 pitch {ppp} px: locate {:.1} ms, read {:.1} ms, body errors {errs}",
            1e3 * t_loc / f64::from(n),
            1e3 * t_read / f64::from(n)
        );
        let _ = run_image;
    }
}

#[test]
#[ignore = "exploratory report"]
fn lens_distortion() {
    for (profile, ppp) in [(Profile::S, 9.0), (Profile::M, 7.0), (Profile::L, 6.0)] {
        for barrel in [0.0, 0.005, 0.01, 0.02, 0.04, -0.01, -0.02] {
            let d = Degrade {
                ppp,
                blur: 1.0,
                noise: 4.0,
                tilt_deg: 10.0,
                barrel,
                border: 2.0,
                ..Degrade::default()
            };
            print(
                &format!("barrel {barrel}"),
                profile,
                &batch(profile, &d, false, &[1, 2]),
            );
        }
    }
}

#[test]
#[ignore = "timing; use --release"]
fn locate_without_a_code_1080p() {
    let busy = common::busy_image(1920, 1080, 4, true);
    let yuv = busy.yuv420();
    let mut rng = common::Rng::new(5);
    let noise = particlecode_vision::Plane::new(
        1920,
        1080,
        (0..1920 * 1080).map(|_| rng.below(256) as u8).collect(),
    );
    for (name, plane) in [("busy", &yuv.y), ("white noise", &noise)] {
        let t = std::time::Instant::now();
        let n = 5;
        for _ in 0..n {
            assert!(locate(plane).is_none());
        }
        eprintln!(
            "locate on {name} 1080p without a code: {:.1} ms",
            1e3 * t.elapsed().as_secs_f64() / f64::from(n)
        );
    }
}

#[test]
#[ignore = "timing; use --release"]
fn locate_and_read_4k() {
    let sc = scene(Profile::M, 5);
    let d = Degrade {
        ppp: 16.0,
        blur: 2.0,
        noise: 3.0,
        tilt_deg: 10.0,
        ..Degrade::default()
    };
    let img = render(&sc, &d);
    let big = paste(
        &img,
        3840,
        2160,
        [60, 62, 66],
        3840 / 2 - img.width / 2,
        1080 - img.height / 2,
    );
    let yuv = big.yuv420();
    let t = std::time::Instant::now();
    let lock = locate(&yuv.y).expect("lock");
    let t_loc = t.elapsed().as_secs_f64();
    let t = std::time::Instant::now();
    let raw = read_with_order(&yuv, &lock, &sc.order).expect("read");
    let errs = symbol_errors(&sc.spec.body_symbols(), &raw.body_symbols);
    eprintln!(
        "4K, pitch {:.1} px: locate {:.1} ms, read {:.1} ms, body errors {errs}",
        lock.pitch_px,
        1e3 * t_loc,
        1e3 * t.elapsed().as_secs_f64()
    );
}

#[test]
#[ignore = "exploratory report"]
fn camera_effects() {
    for (profile, ppp) in [(Profile::M, 7.0), (Profile::L, 6.0)] {
        let base = Degrade {
            ppp,
            blur: 0.9,
            noise: 4.0,
            tilt_deg: 12.0,
            drift: 0.12,
            ..Degrade::default()
        };
        let cases: Vec<(&str, Degrade)> = vec![
            (
                "motion 2px h",
                Degrade {
                    motion: (2.0, 0.0),
                    ..base.clone()
                },
            ),
            (
                "motion 4px h",
                Degrade {
                    motion: (4.0, 0.0),
                    ..base.clone()
                },
            ),
            (
                "motion 3px diag",
                Degrade {
                    motion: (2.1, 2.1),
                    ..base.clone()
                },
            ),
            (
                "shade .15",
                Degrade {
                    shade: 0.15,
                    ..base.clone()
                },
            ),
            (
                "shade .3",
                Degrade {
                    shade: 0.3,
                    ..base.clone()
                },
            ),
            (
                "vignette .3",
                Degrade {
                    vignette: 0.3,
                    ..base.clone()
                },
            ),
            (
                "vignette .5",
                Degrade {
                    vignette: 0.5,
                    ..base.clone()
                },
            ),
            (
                "dof .7->1.8",
                Degrade {
                    blur: 0.7,
                    blur_far: 1.8,
                    ..base.clone()
                },
            ),
            (
                "shade+vig+dof",
                Degrade {
                    shade: 0.2,
                    vignette: 0.3,
                    blur: 0.7,
                    blur_far: 1.5,
                    ..base.clone()
                },
            ),
        ];
        for (name, d) in cases {
            print(name, profile, &batch(profile, &d, false, &[1, 2]));
        }
    }
}

/// Lock rate and symbol error rate over random camera poses.
#[test]
#[ignore = "exploratory report"]
fn random_poses() {
    for (profile, ppp) in PROFILES {
        let mut rng = common::Rng::new(2024);
        let n = 40;
        let (mut locks, mut read_ok, mut worst, mut body_err, mut body_n, mut header_err) =
            (0, 0, 0.0f64, 0, 0, 0);
        let mut unmarked = 0;
        let mut ms = 0.0;
        for k in 0..n {
            let d = Degrade {
                ppp: ppp * (0.85 + 0.35 * rng.f64()),
                blur: 0.4 + 0.8 * rng.f64(),
                noise: 8.0 * rng.f64(),
                tilt_deg: 25.0 * rng.f64(),
                rot_deg: 360.0 * rng.f64(),
                gain: 0.6 + 0.5 * rng.f64(),
                drift: 0.12 * rng.f64(),
                shade: 0.2 * rng.f64(),
                seed: 500 + k,
                ..Degrade::default()
            };
            let sc = scene(profile, 900 + k);
            let img = render(&sc, &d);
            let yuv = img.yuv420();
            let Some(lock) = locate(&yuv.y) else { continue };
            locks += 1;
            let t = std::time::Instant::now();
            if let Ok(raw) = read_with_order(&yuv, &lock, &sc.order) {
                ms += t.elapsed().as_secs_f64() * 1e3;
                read_ok += 1;
                let e = symbol_errors(&sc.spec.body_symbols(), &raw.body_symbols);
                header_err += symbol_errors(&sc.spec.header_symbols(), &raw.header_symbols);
                unmarked += raw
                    .body_symbols
                    .iter()
                    .zip(sc.spec.body_symbols())
                    .filter(|(r, t)| r.value != t.value && r.confidence >= common::ERASURE_BELOW)
                    .count();
                body_err += e;
                body_n += sc.spec.dots.len();
                worst = worst.max(e as f64 / sc.spec.dots.len() as f64);
            }
        }
        eprintln!(
            "{profile:?}: {n} random poses: locks {locks}, reads {read_ok}, header symbol errors {header_err}, body SER {:.3}% (worst frame {:.2}%, unmarked wrong symbols {unmarked}), read {:.0} ms",
            100.0 * body_err as f64 / body_n.max(1) as f64,
            100.0 * worst,
            ms / f64::from(read_ok.max(1))
        );
    }
}
