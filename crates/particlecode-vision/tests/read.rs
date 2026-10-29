//! Symbol-level accuracy of the vision reader on synthetic renders.
//!
//! Every test renders `FrameSpec`s in the exact palette, degrades them like a camera
//! would, finds them with `locate` and compares header symbols, body symbols and particle
//! sectors with the ground truth. `Header::decode` needs the codec work package, so these
//! tests compare *symbols*; the end-to-end `read` test at the bottom is ignored until
//! `particlecode_core::rs` is implemented.
#![allow(clippy::print_stderr)] // diagnostics on failure

mod common;

use common::{Degrade, batch, disc_order, paste, render, run_image, scene, scene_with_order};
use particlecode_core::{Header, Profile};
use particlecode_vision::{VisionError, locate, read, read_with_order, sharpness};

/// The degradation the spec asks for: blur sigma ~ 1 px, noise sigma ~ 4, 15 degrees of tilt,
/// 0.8 gain.
fn spec_degrade(ppp: f64) -> Degrade {
    Degrade {
        ppp,
        blur: 1.0,
        noise: 4.0,
        tilt_deg: 15.0,
        gain: 0.8,
        ..Degrade::default()
    }
}

const PROFILES: [(Profile, f64); 3] = [(Profile::S, 9.0), (Profile::M, 7.0), (Profile::L, 6.0)];

#[test]
fn clean_frames_read_exactly() {
    for (profile, ppp) in PROFILES {
        let b = batch(
            profile,
            &Degrade {
                ppp,
                ..Degrade::default()
            },
            false,
            &[1, 2],
        );
        assert_eq!(b.locks, 2, "{profile:?}: lock");
        assert_eq!(b.header_err, 0, "{profile:?}: header symbols");
        assert!(b.ser() < 0.005, "{profile:?}: SER {}", b.ser());
        for o in &b.outcomes {
            assert_eq!(o.sectors_ok, Some(true), "{profile:?}: particles");
            assert!(o.residual < 0.05, "{profile:?}: residual {}", o.residual);
        }
    }
}

#[test]
fn spec_degradation_stays_under_budget() {
    for (profile, ppp) in PROFILES {
        let b = batch(profile, &spec_degrade(ppp), false, &[1, 2, 3]);
        assert_eq!(b.locks, 3, "{profile:?}: lock");
        assert_eq!(b.header_err, 0, "{profile:?}: header symbols");
        assert!(b.ser() < 0.01, "{profile:?}: SER {}", b.ser());
        for o in &b.outcomes {
            assert_eq!(o.sectors_ok, Some(true), "{profile:?}: particles");
            assert!(o.herr < 0.05, "{profile:?}: homography off by {} p", o.herr);
        }
    }
}

#[test]
fn rotations_and_strong_perspective() {
    let (profile, ppp) = (Profile::M, 7.0);
    for rot in [90.0, 180.0, 270.0, 37.0] {
        let d = Degrade {
            rot_deg: rot,
            ..spec_degrade(ppp)
        };
        let b = batch(profile, &d, false, &[4]);
        assert_eq!(b.locks, 1, "rotation {rot}");
        assert_eq!(b.header_err, 0, "rotation {rot}");
        assert!(b.ser() < 0.01, "rotation {rot}: {}", b.ser());
        assert_eq!(
            b.outcomes[0].sectors_ok,
            Some(true),
            "rotation {rot}: particles"
        );
    }
    let steep = Degrade {
        rot_deg: 20.0,
        tilt_deg: 30.0,
        ..spec_degrade(ppp)
    };
    let b = batch(profile, &steep, false, &[5]);
    assert_eq!(b.locks, 1, "30 degree tilt");
    assert!(b.ser() < 0.02, "30 degree tilt: {}", b.ser());
}

#[test]
fn mirrored_view_is_read_too() {
    // A selfie-camera view: the halo is mirror-symmetric, so `locate` locks as usual and
    // `read_with_order` finds out from the hue order and mirrors the homography.
    for (profile, ppp) in [(Profile::S, 9.0), (Profile::M, 7.0)] {
        let d = Degrade {
            mirror: true,
            rot_deg: 25.0,
            ..spec_degrade(ppp)
        };
        let b = batch(profile, &d, false, &[1, 2]);
        assert_eq!(b.locks, 2, "{profile:?}: lock");
        assert_eq!(b.header_err, 0, "{profile:?}");
        assert!(b.ser() < 0.01, "{profile:?}: {}", b.ser());
        for o in &b.outcomes {
            assert_eq!(o.sectors_ok, Some(true), "{profile:?}: particles");
        }
    }
}

#[test]
fn half_resolution_chroma_is_enough() {
    for (profile, ppp) in [(Profile::S, 9.0), (Profile::M, 7.0)] {
        let b = batch(profile, &spec_degrade(ppp), true, &[1, 2]);
        assert_eq!(b.locks, 2, "{profile:?}: lock");
        assert_eq!(b.header_err, 0, "{profile:?}");
        assert!(b.ser() < 0.01, "{profile:?}: {}", b.ser());
    }
}

#[test]
fn breathing_wave_and_jitter() {
    for (profile, ppp) in [(Profile::M, 7.0), (Profile::L, 6.0)] {
        let wave = Degrade {
            drift: 0.12,
            ..spec_degrade(ppp)
        };
        let b = batch(profile, &wave, false, &[1, 2]);
        assert_eq!(b.locks, 2);
        assert!(b.ser() < 0.01, "{profile:?} wave: {}", b.ser());
        // Independent per-dot jitter is far harsher than the coherent wave.
        let jitter = Degrade {
            jitter: 0.08,
            ..spec_degrade(ppp)
        };
        let b = batch(profile, &jitter, false, &[1, 2]);
        assert_eq!(b.locks, 2);
        assert!(b.ser() < 0.02, "{profile:?} jitter: {}", b.ser());
    }
}

#[test]
fn colour_casts_are_calibrated_away() {
    let base = spec_degrade(7.0);
    for tint in [[1.12, 1.0, 0.78], [0.85, 1.0, 1.15], [1.0, 0.85, 0.55]] {
        let b = batch(
            Profile::M,
            &Degrade {
                tint,
                ..base.clone()
            },
            false,
            &[1],
        );
        assert_eq!(b.locks, 1, "tint {tint:?}");
        assert_eq!(b.header_err, 0, "tint {tint:?}");
        assert!(b.ser() < 0.01, "tint {tint:?}: {}", b.ser());
    }
}

#[test]
fn dim_light_is_fine() {
    let d = Degrade {
        gain: 0.35,
        ..spec_degrade(7.0)
    };
    let b = batch(Profile::M, &d, false, &[1]);
    assert_eq!(b.locks, 1);
    assert!(b.ser() < 0.01, "{}", b.ser());
}

#[test]
fn glare_damage_is_localised_and_mostly_flagged() {
    let d = Degrade {
        glare: Some((10.0, -5.0, 6.0, 0.5)),
        ..spec_degrade(7.0)
    };
    let b = batch(Profile::M, &d, false, &[1, 2]);
    assert_eq!(b.locks, 2);
    assert_eq!(b.header_err, 0, "the halos are away from the glare");
    assert!(b.ser() < 0.08, "{}", b.ser());
    let unmarked: usize = b.outcomes.iter().map(|o| o.unmarked_errors).sum();
    // Reed-Solomon corrects an unflagged error for the price of two erasures, so most of
    // the damage should carry a low confidence.
    assert!(
        unmarked * 3 <= b.body_err * 2,
        "{unmarked} of {} errors unmarked",
        b.body_err
    );
}

#[test]
fn confidence_is_high_on_clean_symbols() {
    let sc = scene(Profile::M, 3);
    let img = render(&sc, &spec_degrade(7.0));
    let yuv = img.yuv();
    let lock = locate(&yuv.y).expect("lock");
    let raw = read_with_order(&yuv, &lock, &sc.order).expect("read");
    let strong = raw
        .body_symbols
        .iter()
        .filter(|s| s.confidence >= 128)
        .count();
    assert!(
        strong * 100 >= raw.body_symbols.len() * 95,
        "{strong} of {} confident",
        raw.body_symbols.len()
    );
}

#[test]
fn code_in_a_big_camera_frame() {
    let sc = scene(Profile::M, 8);
    let img = render(&sc, &spec_degrade(8.0));
    let big = paste(
        &img,
        1600,
        1000,
        [70, 60, 50],
        1600 / 2 - img.width / 2,
        1000 / 2 - img.height / 2,
    );
    let o = run_image(&sc, &big, &big.yuv420()).expect("read");
    assert_eq!(o.header_err, 0);
    assert!(o.ser() < 0.01, "{}", o.ser());
}

#[test]
fn code_cut_by_the_border_is_rejected_not_misread() {
    let sc = scene(Profile::M, 9);
    let img = render(&sc, &spec_degrade(7.0));
    // Keep only the left 62 % of the picture: the lower right anchor and its halo are gone.
    let w = img.width * 62 / 100;
    let mut rgb = Vec::new();
    for y in 0..img.height {
        rgb.extend_from_slice(&img.rgb[y * img.width * 3..(y * img.width + w) * 3]);
    }
    let yuv = particlecode_vision::YuvFrame::from_rgb(w, img.height, &rgb);
    if let Some(lock) = locate(&yuv.y) {
        let r = read_with_order(&yuv, &lock, &sc.order);
        assert!(r.is_err(), "a cut-off code must not read as if complete");
    }
}

#[test]
fn sharpness_falls_with_blur() {
    let sc = scene(Profile::M, 6);
    let mut last = f64::MAX;
    for blur in [0.0, 1.0, 2.0] {
        let img = render(
            &sc,
            &Degrade {
                blur,
                noise: 2.0,
                ..Degrade::default()
            },
        );
        let yuv = img.yuv();
        let lock = locate(&yuv.y).expect("lock");
        let s = sharpness(&yuv.y, &lock);
        assert!(s < last, "blur {blur}: {s} is not below {last}");
        last = s;
    }
}

#[test]
fn rejects_wrong_dot_order_profile() {
    let sc = scene(Profile::M, 2);
    let img = render(&sc, &spec_degrade(7.0));
    let yuv = img.yuv();
    let lock = locate(&yuv.y).expect("lock");
    let wrong = disc_order(Profile::S);
    assert_eq!(
        read_with_order(&yuv, &lock, &wrong).err(),
        Some(VisionError::ProfileMismatch)
    );
}

#[test]
fn unusable_frames_do_not_panic() {
    let sc = scene(Profile::M, 2);
    let img = render(&sc, &spec_degrade(7.0));
    let yuv = img.yuv();
    let lock = locate(&yuv.y).expect("lock");
    // Luma only, mismatched chroma planes, a tiny image: all rejected.
    let tiny = particlecode_vision::YuvFrame::from_rgb(8, 8, &[100; 192]);
    assert!(read_with_order(&tiny, &lock, &sc.order).is_err());
    let mut broken = yuv.clone();
    broken.u.data.truncate(10);
    assert!(read_with_order(&broken, &lock, &sc.order).is_err());
    assert!(locate(&broken.y).is_some());
}

/// End to end through `Header::decode`; needs `particlecode_core::rs` (header RS code).
#[test]
#[ignore = "needs particlecode_core::rs::{header_encode, header_decode} from the codec work package"]
fn full_read_decodes_the_header_and_picks_the_order() {
    for (profile, ppp) in PROFILES {
        let order = disc_order(profile);
        let mut sc = scene_with_order(profile, order.clone(), 11);
        // Re-assemble with a real, RS-encoded header.
        let header: Header = sc.spec.header;
        let sectors: [u8; 6] = std::array::from_fn(|k| sc.spec.particles[k].sector);
        let body: Vec<u8> = sc.spec.body_symbols().iter().map(|s| s.value).collect();
        sc.spec = particlecode_core::FrameSpec::assemble(
            0,
            header,
            &header.encode(),
            &order,
            &body,
            sectors,
        );
        let img = render(&sc, &spec_degrade(ppp));
        let yuv = img.yuv();
        let lock = locate(&yuv.y).expect("lock");
        let order_for = |p: Profile, shape: u8| (p == profile && shape == 0).then(|| order.clone());
        let (h, symbols, liveness) = read(&yuv, &lock, &order_for).expect("read");
        assert_eq!(h, header);
        assert_eq!(symbols.len(), order.len());
        let want =
            particlecode_vision::Liveness::from_sectors(Some(sectors), Some(header.packet_seed));
        assert_eq!(liveness, want);
    }
}

#[test]
fn non_disc_shapes_read_too() {
    use common::{Shape, shape_order};
    for shape in [Shape::RoundedSquare, Shape::T, Shape::Plus, Shape::Ring] {
        let profile = Profile::M;
        let order = shape_order(profile, shape);
        assert!(order.len() > 500, "{shape:?}: {} dots", order.len());
        let sc = scene_with_order(profile, order, 31);
        let o = common::run(&sc, &spec_degrade(6.5), false)
            .unwrap_or_else(|e| panic!("{shape:?}: {e}"));
        assert_eq!(o.header_err, 0, "{shape:?}");
        assert!(o.ser() < 0.01, "{shape:?}: SER {}", o.ser());
        assert_eq!(o.sectors_ok, Some(true), "{shape:?}: particles");
    }
}

/// The whole receive path: encoded body and header -> pixels -> `locate` -> `read` ->
/// `BodyCodec::decode`, including a glare patch that RS has to repair with erasures.
///
/// `BodyCodec` does not exist in the scaffold this crate was written against, so the test is
/// compiled out. After the codec work package is merged, delete the `cfg(any())` line below
/// (verified to pass against that package's `body.rs` and `rs.rs`).
#[cfg(any())]
#[test]
fn end_to_end_through_the_real_codecs() {
    use common::Rng;
    use particlecode_core::body::{BodyCodec, WhitenKey};
    use particlecode_core::{EccLevel, FrameSpec};

    for (profile, ppp, glare) in [
        (Profile::S, 9.0, None),
        (Profile::M, 7.0, None),
        (Profile::L, 6.0, None),
        (Profile::M, 7.0, Some((10.0, -5.0, 6.0, 0.5))),
    ] {
        let order = disc_order(profile);
        let codec = BodyCodec::new(order.len(), EccLevel::Normal);
        let mut rng = Rng::new(77);
        let data: Vec<u8> = (0..codec.block_size())
            .map(|_| rng.below(256) as u8)
            .collect();
        let mut sc = scene_with_order(profile, order.clone(), 21);
        let header = Header {
            session_id: 0xBEEF,
            segment_index: 0,
            packet_seed: 7,
            ..sc.spec.header
        };
        let key = WhitenKey {
            session_id: 0xBEEF,
            segment_index: 0,
            packet_seed: 7,
        };
        let sectors: [u8; 6] = std::array::from_fn(|k| sc.spec.particles[k].sector);
        sc.spec = FrameSpec::assemble(
            0,
            header,
            &header.encode(),
            &order,
            &codec.encode(&data, key),
            sectors,
        );

        let img = render(
            &sc,
            &Degrade {
                glare,
                ..spec_degrade(ppp)
            },
        );
        let yuv = img.yuv420();
        let lock = locate(&yuv.y).expect("lock");
        let order_for = |p: Profile, shape: u8| (p == profile && shape == 0).then(|| order.clone());
        let (h, symbols, _) = read(&yuv, &lock, &order_for).expect("read");
        assert_eq!(h, header, "{profile:?}");
        let decoded = codec
            .decode(&symbols, key)
            .unwrap_or_else(|e| panic!("{profile:?} {glare:?}: {e}"));
        assert_eq!(decoded, data, "{profile:?} {glare:?}");
    }
}

#[test]
fn lighting_gradient_and_vignetting() {
    // A screen seen at an angle is brighter on one side; lenses darken the corners. The
    // paper level is fitted with a plane and divided out.
    for (profile, ppp) in [(Profile::M, 7.0), (Profile::L, 6.0)] {
        let d = Degrade {
            shade: 0.25,
            vignette: 0.25,
            ..spec_degrade(ppp)
        };
        let b = batch(profile, &d, false, &[1, 2]);
        assert_eq!(b.locks, 2, "{profile:?}: lock");
        assert!(
            b.header_err <= 2,
            "{profile:?}: header errors {}",
            b.header_err
        );
        assert!(b.ser() < 0.02, "{profile:?}: {}", b.ser());
    }
}

#[test]
fn motion_blur_and_depth_of_field() {
    let (profile, ppp) = (Profile::M, 7.0);
    let motion = Degrade {
        motion: (3.0, 1.0),
        ..spec_degrade(ppp)
    };
    let b = batch(profile, &motion, false, &[1, 2]);
    assert_eq!(b.locks, 2);
    assert!(b.ser() < 0.02, "motion: {}", b.ser());
    // Sharp at the top of the picture, soft at the bottom.
    let dof = Degrade {
        blur: 0.7,
        blur_far: 1.7,
        ..spec_degrade(ppp)
    };
    let b = batch(profile, &dof, false, &[1, 2]);
    assert_eq!(b.locks, 2);
    assert!(b.ser() < 0.02, "depth of field: {}", b.ser());
}

#[test]
fn lens_distortion_is_absorbed_by_per_dot_refinement() {
    let d = Degrade {
        barrel: 0.02,
        border: 2.0,
        ..spec_degrade(7.0)
    };
    let b = batch(Profile::M, &d, false, &[1, 2]);
    assert_eq!(b.locks, 2);
    assert_eq!(b.header_err, 0);
    assert!(b.ser() < 0.01, "{}", b.ser());
}

#[test]
fn an_order_without_dots_reads_an_empty_body() {
    let sc = scene(Profile::S, 3);
    let img = render(&sc, &spec_degrade(9.0));
    let yuv = img.yuv();
    let lock = locate(&yuv.y).expect("lock");
    let lattice = particlecode_core::layout::Lattice::new(Profile::S);
    let empty = particlecode_core::layout::DotOrder::new(&lattice, &vec![false; lattice.len()]);
    let raw = read_with_order(&yuv, &lock, &empty).expect("the halo alone is readable");
    assert!(raw.body_symbols.is_empty());
    assert_eq!(raw.header_symbols.len(), 48);
}
