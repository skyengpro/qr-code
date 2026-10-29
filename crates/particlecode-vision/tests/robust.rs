//! The decoder must reject, never panic: random pixels, inconsistent planes, absurd locks.

mod common;

use std::sync::OnceLock;

use common::{Degrade, Rendered, Scene, disc_order, lock_from_truth, render, scene};
use particlecode_core::Profile;
use particlecode_vision::{Homography, Lock, Plane, YuvFrame, locate, read_with_order, sharpness};
use proptest::prelude::*;

fn fixture() -> &'static (Scene, Rendered, YuvFrame) {
    static F: OnceLock<(Scene, Rendered, YuvFrame)> = OnceLock::new();
    F.get_or_init(|| {
        let sc = scene(Profile::S, 17);
        let img = render(
            &sc,
            &Degrade {
                ppp: 6.0,
                blur: 0.8,
                noise: 3.0,
                tilt_deg: 10.0,
                ..Degrade::default()
            },
        );
        let yuv = img.yuv();
        (sc, img, yuv)
    })
}

fn any_profile() -> impl Strategy<Value = Profile> {
    prop_oneof![Just(Profile::S), Just(Profile::M), Just(Profile::L)]
}

fn any_f64() -> impl Strategy<Value = f64> {
    prop_oneof![
        -1e4..1e4f64,
        Just(0.0),
        Just(f64::NAN),
        Just(f64::INFINITY),
        Just(f64::NEG_INFINITY),
        Just(1e-300),
        Just(1e300),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn locate_never_panics_on_random_planes(
        width in 0usize..96,
        height in 0usize..96,
        extra_stride in 0usize..8,
        short in 0usize..3,
        seed in any::<u64>(),
    ) {
        let stride = width + extra_stride;
        let mut rng = common::Rng::new(seed);
        let len = (stride * height).saturating_sub(short * 40);
        let data: Vec<u8> = (0..len).map(|_| rng.below(256) as u8).collect();
        let plane = Plane::with_stride(width, height, stride, data);
        if let Some(lock) = locate(&plane) {
            prop_assert!(lock.pitch_px.is_finite());
        }
    }

    #[test]
    fn locate_never_panics_on_blobby_images(seed in any::<u64>(), w in 48usize..200, h in 48usize..200) {
        // Random discs and rings of random darkness: plenty of components, no code.
        let mut rng = common::Rng::new(seed);
        let mut data = vec![230u8; w * h];
        for _ in 0..rng.below(60) {
            let (cx, cy) = (rng.f64() * w as f64, rng.f64() * h as f64);
            let r = 1.0 + rng.f64() * 15.0;
            let dark = rng.below(120) as u8;
            let hole = rng.f64() * r * rng.below(2) as f64;
            for y in 0..h {
                for x in 0..w {
                    let d = (x as f64 - cx).hypot(y as f64 - cy);
                    if d <= r && d >= hole {
                        data[y * w + x] = dark;
                    }
                }
            }
        }
        let plane = Plane::new(w, h, data);
        let _ = locate(&plane);
    }

    #[test]
    fn read_never_panics_on_absurd_locks(
        profile in any_profile(),
        entries in proptest::array::uniform9(any_f64()),
        anchors in proptest::array::uniform6(any_f64()),
        pitch in any_f64(),
        perturb in 0.0..0.3f64,
        mode in 0u8..3,
    ) {
        let (sc, img, yuv) = fixture();
        let truth = lock_from_truth(Profile::S, &img.truth);
        let homography = match mode {
            // Random garbage.
            0 => Homography([
                [entries[0], entries[1], entries[2]],
                [entries[3], entries[4], entries[5]],
                [entries[6], entries[7], entries[8]],
            ]),
            // The right map, scaled and sheared a little.
            1 => {
                let mut m = img.truth.0;
                for (i, row) in m.iter_mut().enumerate() {
                    for (j, c) in row.iter_mut().enumerate() {
                        *c *= 1.0 + perturb * ((i * 3 + j) as f64 - 4.0) / 4.0;
                    }
                }
                Homography(m)
            }
            // Right up to a translation of up to a few pitches.
            _ => {
                let mut m = img.truth.0;
                m[0][2] += perturb * 100.0;
                m[1][2] -= perturb * 100.0;
                Homography(m)
            }
        };
        let lock = Lock {
            profile,
            homography,
            anchors_px: [(anchors[0], anchors[1]), (anchors[2], anchors[3]), (anchors[4], anchors[5])],
            center_px: None,
            pitch_px: pitch,
            ..truth
        };
        let order = disc_order(profile);
        let _ = read_with_order(yuv, &lock, &order);
        let _ = read_with_order(yuv, &lock, &sc.order);
        let _ = sharpness(&yuv.y, &lock);
    }

    #[test]
    fn random_dot_orders_never_panic(density in 0.0..1.0f64, seed in any::<u64>()) {
        // Sparse, patchy, empty or full masks over the S lattice.
        let (_, img, yuv) = fixture();
        let lock = lock_from_truth(Profile::S, &img.truth);
        let lattice = particlecode_core::layout::Lattice::new(Profile::S);
        let mut rng = common::Rng::new(seed);
        let mask: Vec<bool> = lattice.points().iter().map(|_| rng.f64() < density).collect();
        let order = particlecode_core::layout::DotOrder::new(&lattice, &mask);
        if let Ok(raw) = read_with_order(yuv, &lock, &order) {
            prop_assert_eq!(raw.body_symbols.len(), order.len());
        }
    }

    #[test]
    fn occluded_and_cropped_codes_never_panic(
        boxes in proptest::collection::vec((0usize..400, 0usize..400, 1usize..160, 1usize..160, any::<u8>()), 0..6),
        crop_left in 0usize..200,
        crop_top in 0usize..200,
        keep_w in 40usize..500,
        keep_h in 40usize..500,
    ) {
        let (sc, img, yuv) = fixture();
        let mut y = yuv.y.clone();
        for &(bx, by, bw, bh, v) in &boxes {
            for row in by..(by + bh).min(y.height) {
                for col in bx..(bx + bw).min(y.width) {
                    y.data[row * y.stride + col] = v;
                }
            }
        }
        // Crop (keeping the planes consistent with each other).
        let (x0, y0) = (crop_left.min(y.width - 1), crop_top.min(y.height - 1));
        let (w, h) = (keep_w.min(y.width - x0), keep_h.min(y.height - y0));
        let mut crop = Vec::with_capacity(w * h * 3);
        for row in y0..y0 + h {
            for col in x0..x0 + w {
                let g = y.data[row * y.stride + col];
                crop.extend_from_slice(&[g, g, g]);
            }
        }
        let frame = YuvFrame::from_rgb(w, h, &crop);
        if let Some(lock) = locate(&frame.y) {
            let _ = read_with_order(&frame, &lock, &sc.order);
            let _ = sharpness(&frame.y, &lock);
        }
        let lock = lock_from_truth(Profile::S, &img.truth);
        let _ = read_with_order(&frame, &lock, &sc.order);
    }

    #[test]
    fn read_never_panics_on_inconsistent_planes(
        uw in 0usize..400,
        uh in 0usize..400,
        vw in 0usize..400,
        vh in 0usize..400,
        cut in 0usize..3,
    ) {
        let (sc, img, yuv) = fixture();
        let lock = lock_from_truth(Profile::S, &img.truth);
        let mut f = yuv.clone();
        f.u = Plane::new(uw, uh, vec![128; uw * uh]);
        f.v = Plane::new(vw, vh, vec![128; vw * vh]);
        if cut == 1 {
            f.y.data.truncate(f.y.data.len() / 2);
        }
        if cut == 2 {
            f.y.stride += 3;
        }
        let _ = read_with_order(&f, &lock, &sc.order);
        let _ = locate(&f.y);
    }
}

#[test]
fn yuv_from_rgb_tolerates_short_buffers() {
    let f = YuvFrame::from_rgb(10, 10, &[1, 2, 3]);
    assert_eq!(f.y.width, 0);
    let f = YuvFrame::from_rgb(0, 5, &[]);
    assert_eq!(f.y.width, 0);
}
