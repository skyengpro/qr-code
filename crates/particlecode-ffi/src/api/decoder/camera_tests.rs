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
            ProgressDto::Accepted { .. } => assert!(r.reject_reason.is_none()),
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
    }
    assert_eq!(dec.session_id(), None);
}

#[test]
fn lighting_failures_map_to_a_user_hint() {
    let hint = vision_hint(&VisionError::Lighting);
    assert!(hint.contains("dark") && hint.contains("glare"));
    assert!(vision_hint(&VisionError::Truncated).contains("back"));
    assert_eq!(vision_hint(&VisionError::UnknownShape(9)), UPDATE_HINT);
}
