//! Symbol-level end to end: `Session` frames → (corrupted) symbols → `Receiver` → payload.

use particlecode_core::shapes::{self, ShapeTable};
use particlecode_core::{
    DecodeError, DisplayInfo, EncodeError, Profile, Progress, Receiver, Session, SessionConfig,
    ShapeChoice, Sym3,
};
use proptest::prelude::*;

const DISPLAY: DisplayInfo = DisplayInfo {
    width_px: 1080,
    height_px: 2400,
    width_mm: 68.0,
    height_mm: 151.0,
};

fn payload(n: usize, seed: u32) -> Vec<u8> {
    let mut x = seed | 1;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x >> 8) as u8
        })
        .collect()
}

fn session(data: &[u8], config: &SessionConfig) -> Session {
    Session::with_identity(data, DISPLAY, config, 0x5EED, [7; 32], 1_700_000_000).unwrap()
}

/// Feed frames until done; returns (payload, frames fed).
fn receive(
    session: &Session,
    mut corrupt: impl FnMut(u32, &mut [Sym3]),
    skip: impl Fn(u32) -> bool,
) -> (Vec<u8>, u32) {
    let mut rx = Receiver::new();
    let mut streamed = Vec::new();
    for i in 0..session.frames_per_pass() * 6 {
        if skip(i) {
            continue;
        }
        let mut symbols = session.frame(i).all_symbols();
        corrupt(i, &mut symbols);
        let progress = rx.feed(&symbols);
        if let Some(bytes) = rx.take_ready() {
            streamed.extend(bytes);
        }
        if let Progress::Done(data) = progress {
            assert_eq!(
                streamed, data,
                "streamed segments must equal the final payload"
            );
            return (data, i + 1);
        }
    }
    panic!("transfer did not complete");
}

#[test]
fn clean_transfer_takes_exactly_k_frames_per_segment() {
    let data = payload(6000, 1);
    let s = session(&data, &SessionConfig::default());
    let (got, frames) = receive(&s, |_, _| {}, |_| false);
    assert_eq!(got, data);
    // 6 KB at profile M: one 8 KB segment, K = ceil(6008 / T) source symbols.
    let k = 6008usize.div_ceil(s.block_size()) as u32;
    assert_eq!(frames, k);
}

#[test]
fn every_profile_and_some_shapes() {
    for profile in Profile::ALL {
        for shape in [shapes::DISC, shapes::HEART, shapes::letter('A').unwrap()] {
            let config = SessionConfig {
                profile: Some(profile),
                shape: ShapeChoice::Require(shape),
                ..SessionConfig::default()
            };
            let data = payload(3000, u32::from(shape) + 7);
            let s = session(&data, &config);
            assert_eq!(s.frame(0).header.shape_id, shape);
            assert_eq!(
                receive(&s, |_, _| {}, |_| false).0,
                data,
                "{profile:?} shape {shape}"
            );
        }
    }
}

#[test]
fn survives_symbol_errors_erasures_and_dropped_frames() {
    let data = payload(20_000, 9);
    let s = session(&data, &SessionConfig::default());
    let mut x = 12345u32;
    let mut rnd = move || {
        x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        x >> 8
    };
    let (got, _) = receive(
        &s,
        |_, syms| {
            for sym in syms.iter_mut() {
                match rnd() % 1000 {
                    // 1.5 % confident errors.
                    0..15 => sym.value = (sym.value + 1 + (rnd() % 7) as u8) % 8,
                    // 4 % flagged low-confidence (half of them wrong).
                    15..55 => {
                        sym.confidence = 10;
                        if rnd() % 2 == 0 {
                            sym.value ^= 3;
                        }
                    }
                    _ => {}
                }
            }
        },
        |i| i % 4 == 1,
    );
    assert_eq!(got, data);
}

#[test]
fn static_frame_and_shape_fallback() {
    let small = payload(400, 3);
    let config = SessionConfig {
        is_static: true,
        shape: ShapeChoice::Require(shapes::HEART),
        ..SessionConfig::default()
    };
    let s = session(&small, &config);
    assert_eq!(s.frames_per_pass(), 1);
    assert!(s.frame(0).header.is_static);
    assert_eq!(receive(&s, |_, _| {}, |_| false), (small, 1));

    // Too big for one letter-I frame: Require fails, Prefer falls back to the disc.
    let big = payload(1000, 4);
    let i = shapes::letter('I').unwrap();
    let require = SessionConfig {
        shape: ShapeChoice::Require(i),
        ..config.clone()
    };
    assert!(matches!(
        Session::with_identity(&big, DISPLAY, &require, 1, [0; 32], 0),
        Err(EncodeError::DoesNotFit { .. })
    ));
    let prefer = SessionConfig {
        shape: ShapeChoice::Prefer(i),
        ..config.clone()
    };
    let s = Session::with_identity(&big, DISPLAY, &prefer, 1, [0; 32], 0).unwrap();
    assert_eq!(s.frame(0).header.shape_id, shapes::DISC);
    let auto = SessionConfig {
        shape: ShapeChoice::Auto(vec![i, shapes::HEART, shapes::ROUNDED_SQUARE]),
        ..config
    };
    let s = Session::with_identity(&big, DISPLAY, &auto, 1, [0; 32], 0).unwrap();
    // The heart holds ~776 bytes at static ECC; the rounded square is next in line.
    assert_eq!(s.frame(0).header.shape_id, shapes::ROUNDED_SQUARE);
}

#[test]
fn unknown_shape_is_rejected_on_both_ends() {
    let config = SessionConfig {
        shape: ShapeChoice::Require(200),
        ..SessionConfig::default()
    };
    assert_eq!(
        Session::with_identity(b"x", DISPLAY, &config, 1, [0; 32], 0).unwrap_err(),
        EncodeError::UnknownShape(200)
    );

    // A sender with a custom shape; a receiver without it rejects, one with it decodes.
    let mut table = ShapeTable::builtin();
    let masks = Profile::ALL.map(|p| table.mask(shapes::HEXAGON, p).unwrap());
    table.insert(200, "custom-hexagon", masks).unwrap();
    let data = payload(2000, 5);
    let s = Session::with_shapes(&data, DISPLAY, &config, &table, 9, [1; 32], 0).unwrap();
    let frame = s.frame(0).all_symbols();
    assert!(matches!(
        Receiver::new().feed(&frame),
        Progress::Rejected(DecodeError::UnknownShape(200))
    ));
    assert!(matches!(
        Receiver::with_shapes(table).feed(&frame),
        Progress::Accepted { .. }
    ));
}

#[test]
fn frames_are_deterministic() {
    let data = payload(9000, 8);
    let a = session(&data, &SessionConfig::default());
    let b = session(&data, &SessionConfig::default());
    for i in [0, 1, 5, 17, 40] {
        assert_eq!(a.frame(i), b.frame(i));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn roundtrip_any_payload(len in 0usize..12_000, seed in any::<u32>(), drop_every in 2u32..6) {
        let data = payload(len, seed);
        let s = session(&data, &SessionConfig::default());
        let (got, _) = receive(&s, |_, _| {}, |i| i % drop_every == 0);
        prop_assert_eq!(got, data);
    }

    #[test]
    fn garbage_never_panics(values in proptest::collection::vec(0u8..8, 0..5000)) {
        let symbols: Vec<Sym3> = values.into_iter().map(Sym3::new).collect();
        let _ = Receiver::new().feed(&symbols);
    }
}
