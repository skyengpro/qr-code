//! Camera path: synthetic photos of real frames through `feed_yuv` / `feed_rgba`.

use super::*;
use crate::api::dto::ProfileDto;
use crate::testutil::{camera_rgb, encoder, i420, nv21, payload, rgba};

const SIZE_PX: u32 = 760;

/// Feed camera frames from `feed` until `Done`; checks every per-frame result.
fn run(data: &[u8], mut feed: impl FnMut(&mut ParticleDecoder, u32) -> FrameResultDto) {
    let frames_per_pass = encoder(data).frames_per_pass();
    let mut dec = ParticleDecoder::new();
    let mut streamed = Vec::new();
    for i in 0..frames_per_pass * 3 {
        let r = feed(&mut dec, i);
        assert!(r.locked, "frame {i}: not locked: {:?}", r.reject_reason);
        assert!(r.anchors.len() >= 3);
        assert!(r.sharpness > 0.0);
        assert_eq!(r.profile, Some(ProfileDto::M));
        assert_eq!(r.shape_id, Some(0));
        assert!(
            matches!(
                r.liveness,
                LivenessDto::Read {
                    consistent: true,
                    ..
                }
            ),
            "{:?}",
            r.liveness
        );
        if let Some(bytes) = dec.take_ready() {
            streamed.extend(bytes);
        }
        match r.progress {
            ProgressDto::Done { payload } => {
                assert_eq!(payload, data);
                assert_eq!(streamed, data);
                assert_eq!(dec.session_id(), Some(0xC0DE));
                return;
            }
            ProgressDto::Accepted { .. } => {
                assert!(r.reject_reason.is_none());
                assert_eq!((r.hint, r.hint_code), (None, HintDto::None));
            }
            ProgressDto::Rejected { reason } => panic!("frame {i} rejected: {reason}"),
        }
    }
    panic!("transfer did not complete");
}

#[test]
fn nv21_with_padded_strides_completes_a_session() {
    let data = payload(2600);
    let enc = encoder(&data);
    run(&data, |dec, i| {
        let p = nv21(&camera_rgb(&enc, i, SIZE_PX), 8);
        assert_eq!(p.uv_pixel_stride, 2);
        assert!(p.y_stride > p.width);
        // The Vec-taking bridge entry point.
        dec.feed_yuv(
            p.width,
            p.height,
            p.y,
            p.u,
            p.v,
            p.y_stride,
            p.uv_stride,
            p.uv_pixel_stride,
        )
    });
}

#[test]
fn planar_i420_completes_a_session() {
    let data = payload(2600);
    let enc = encoder(&data);
    run(&data, |dec, i| {
        let p = i420(&camera_rgb(&enc, i, SIZE_PX), 6);
        dec.feed_yuv_slices(
            p.width,
            p.height,
            (&p.y, &p.u, &p.v),
            (p.y_stride, p.uv_stride, p.uv_pixel_stride),
        )
    });
}

#[test]
fn rgba_completes_a_session() {
    let data = payload(1200);
    let enc = encoder(&data);
    run(&data, |dec, i| {
        let img = camera_rgb(&enc, i, SIZE_PX);
        dec.feed_rgba(img.width, img.height, rgba(&img))
    });
}

#[test]
fn ring_credential_completes_over_multiple_passes_with_missed_frames() {
    use crate::api::dto::{EncoderConfig, ShapeChoiceDto};
    use crate::api::encoder::ParticleEncoder;

    let data = payload(6000);
    let enc = ParticleEncoder::new(
        data.clone(),
        EncoderConfig {
            shape: ShapeChoiceDto::Require { id: 30 },
            profile: Some(ProfileDto::S),
            session_id: Some(0xC0DE),
            key: Some(vec![3; 32]),
            clock_origin_unix: Some(1_700_000_000),
            ..EncoderConfig::default()
        },
    )
    .unwrap();
    let mut dec = ParticleDecoder::new();
    let mut previous_have = 0;
    for i in 0..enc.frames_per_pass() * 5 {
        if i % 3 == 1 {
            continue;
        }
        let img = camera_rgb(&enc, i, SIZE_PX);
        let result = dec.feed_rgba(img.width, img.height, rgba(&img));
        match result.progress {
            ProgressDto::Done { payload } => {
                assert!(
                    i >= enc.frames_per_pass(),
                    "test must cross a pass boundary"
                );
                assert_eq!(payload, data);
                return;
            }
            ProgressDto::Rejected { reason } => panic!("frame {i} rejected: {reason}"),
            ProgressDto::Accepted { have, .. } => {
                assert!(have >= previous_have, "progress fell on frame {i}");
                previous_have = have;
            }
        }
    }
    panic!("ring transfer did not complete");
}

#[test]
fn unusable_frames_are_rejections_with_hints() {
    let mut dec = ParticleDecoder::new();
    let grey = vec![128u8; 300 * 200];
    let half = vec![128u8; 150 * 100];
    let cases: Vec<FrameResultDto> = vec![
        // A blank image: valid buffers, nothing to find.
        dec.feed_yuv_slices(300, 200, (&grey, &half, &half), (300, 150, 1)),
        dec.feed_rgba(40, 40, vec![90; 40 * 40 * 4]),
        // Inconsistent buffers.
        dec.feed_yuv_slices(300, 200, (&grey[..100], &half, &half), (300, 150, 1)),
        dec.feed_yuv_slices(300, 200, (&grey, &half, &half), (299, 150, 1)),
        dec.feed_yuv_slices(300, 200, (&grey, &half, &half), (300, 150, 0)),
        dec.feed_yuv_slices(300, 200, (&grey, &half, &half), (300, 150, 7)),
        dec.feed_yuv_slices(0, 0, (&[], &[], &[]), (0, 0, 1)),
        dec.feed_yuv_slices(u32::MAX, u32::MAX, (&grey, &half, &half), (u32::MAX, 1, 2)),
        dec.feed_rgba(10, 10, vec![0; 5]),
        dec.feed_rgba(u32::MAX, u32::MAX, vec![]),
    ];
    for r in cases {
        assert!(!r.locked);
        assert!(r.anchors.is_empty());
        assert_eq!(r.liveness, LivenessDto::Unknown);
        assert!(matches!(r.progress, ProgressDto::Rejected { .. }));
        assert!(r.reject_reason.is_some());
        assert!(r.hint.is_some());
        assert_ne!(r.hint_code, HintDto::None);
    }
    assert_eq!(dec.session_id(), None);
}

#[test]
fn lighting_failures_map_to_a_user_hint() {
    let hint = vision_hint(&VisionError::Lighting);
    assert_eq!(hint, HintDto::Lighting);
    let text = hint.text().unwrap();
    assert!(text.contains("dark") && text.contains("glare"));
    assert!(
        vision_hint(&VisionError::Truncated)
            .text()
            .unwrap()
            .contains("back")
    );
    assert_eq!(
        vision_hint(&VisionError::UnknownShape(9)),
        HintDto::UpdateApp
    );
    assert_eq!(HintDto::None.text(), None);
}

/// The pointer-handoff entry point: planes passed by address, `U` and `V` aliasing one
/// interleaved `VU` buffer as Android's `YUV_420_888` does, read in place.
#[test]
fn camera_frame_by_address_completes_a_session() {
    use crate::api::dto::CameraFrameDto;
    let data = payload(2600);
    let enc = encoder(&data);
    run(&data, |dec, i| {
        let p = nv21(&camera_rgb(&enc, i, SIZE_PX), 8);
        let vu = &p.v;
        let frame = CameraFrameDto {
            width: p.width,
            height: p.height,
            y_address: p.y.as_ptr() as usize,
            y_len: p.y.len(),
            v_address: vu.as_ptr() as usize,
            v_len: vu.len(),
            u_address: vu.as_ptr() as usize + 1,
            u_len: vu.len() - 1,
            y_stride: p.y_stride,
            uv_stride: p.uv_stride,
            uv_pixel_stride: p.uv_pixel_stride,
        };
        dec.feed_camera_frame(frame)
    });
}

/// Null addresses, empty planes and planes shorter than the dimensions say are rejected
/// before anything is read.
#[test]
fn unusable_camera_frames_are_rejected_before_reading() {
    use crate::api::dto::CameraFrameDto;
    let luma = vec![0u8; 64 * 64];
    let chroma = vec![0u8; 32 * 32];
    let good = CameraFrameDto {
        width: 64,
        height: 64,
        y_address: luma.as_ptr() as usize,
        y_len: luma.len(),
        u_address: chroma.as_ptr() as usize,
        u_len: chroma.len(),
        v_address: chroma.as_ptr() as usize,
        v_len: chroma.len(),
        y_stride: 64,
        uv_stride: 32,
        uv_pixel_stride: 1,
    };
    let mut dec = ParticleDecoder::new();
    for bad in [
        CameraFrameDto {
            y_address: 0,
            ..good
        },
        CameraFrameDto { u_len: 0, ..good },
        // Claims a taller image than the luma buffer holds.
        CameraFrameDto { height: 65, ..good },
        CameraFrameDto {
            y_len: usize::MAX,
            ..good
        },
    ] {
        let r = dec.feed_camera_frame(bad);
        assert!(!r.locked);
        assert!(
            matches!(r.progress, ProgressDto::Rejected { .. }),
            "{bad:?}"
        );
    }
    // The well-formed (blank) frame is read and simply finds no code.
    let r = dec.feed_camera_frame(good);
    assert!(matches!(r.progress, ProgressDto::Rejected { .. }));
}
