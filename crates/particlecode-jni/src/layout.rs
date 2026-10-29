//! The wire contract between Rust and `NativeBridge.kt`: status codes and the layout of the
//! two preallocated result arrays.
//!
//! Everything here is plain constants and safe packing code. The Kotlin side repeats the
//! constants in `NativeBridge.Layout`; keep the two in step (the JVM integration test reads
//! the same indices through the Java harness).
//!
//! # Status codes
//!
//! `nativeFeedYuv` and `nativeFeedYuvBytes` return one of:
//!
//! | value | name | meaning |
//! | --- | --- | --- |
//! | 0 | [`STATUS_REJECTED`] | the frame was not used (no code in view, unreadable, ...) |
//! | 1 | [`STATUS_ACCEPTED`] | the frame was accepted; progress is in the arrays |
//! | 2 | [`STATUS_DONE`] | the whole payload arrived; fetch it with `nativePayload` |
//! | -1 | [`ERR_NULL`] | null or freed handle, or a null buffer or array argument |
//! | -2 | [`ERR_PANIC`] | a panic was caught at the boundary (a bug); discard the handle |
//! | -3 | [`ERR_INVALID`] | a size, stride or array length does not fit the arguments |
//! | -4 | [`ERR_NOT_DIRECT`] | a `ByteBuffer` argument is not a direct buffer |
//! | -5 | [`ERR_JNI`] | a JNI call failed (out of memory, pending exception, ...) |
//!
//! On a negative status the result arrays are left untouched.
//!
//! # `out`: `int[OUT_LEN]`
//!
//! | index | name | value |
//! | --- | --- | --- |
//! | 0 | [`OUT_LOCKED`] | 1 if the three anchors were found, else 0 |
//! | 1 | [`OUT_PROFILE`] | 0 S, 1 M, 2 L; -1 if the header was not read |
//! | 2 | [`OUT_SHAPE`] | shape id of the header, -1 if not read |
//! | 3 | [`OUT_HAVE`] | distinct symbols held, summed over segments |
//! | 4 | [`OUT_NEED`] | symbols needed if nothing is lost |
//! | 5 | [`OUT_SEGMENTS_DONE`] | segments decoded and verified |
//! | 6 | [`OUT_SEGMENTS_TOTAL`] | segments in the transfer |
//! | 7 | [`OUT_LIVENESS_READ`] | 1 if the particles were read, else 0 |
//! | 8 | [`OUT_LIVENESS_CONSISTENT`] | 1 if the particles' seed byte matches the header |
//! | 9 | [`OUT_SEED_LOW`] | low byte of the packet seed shown by the particles (0..=255) |
//! | 10 | [`OUT_ROLLING`] | rolling HMAC value shown by the particles (0..=65535) |
//! | 11 | [`OUT_ANCHOR_COUNT`] | valid anchors in `outGeom`: 0, 3 or 4 |
//! | 12 | [`OUT_HINT`] | hint code (`HINT_*`); `nativeHint` has the text |
//!
//! The counts (3..=6) are the receiver's progress after the frame; they keep their last
//! value across rejected frames so a progress bar does not flicker back to zero.
//!
//! # `outGeom`: `float[GEOM_LEN]`
//!
//! | index | name | value |
//! | --- | --- | --- |
//! | 0..8 | [`GEOM_ANCHORS`] | anchors in image pixels as `x, y` pairs: top, lower right, lower left, center; unused pairs are 0 |
//! | 8 | [`GEOM_SHARPNESS`] | sharpness score (higher is sharper), 0 if not locked |

use particlecode_ffi::api::dto::{FrameResultDto, LivenessDto, ProfileDto, ProgressDto};

/// Status: the frame was rejected.
pub const STATUS_REJECTED: i32 = 0;
/// Status: the frame was accepted.
pub const STATUS_ACCEPTED: i32 = 1;
/// Status: the whole payload has arrived.
pub const STATUS_DONE: i32 = 2;

/// Error: null or freed handle, or a null buffer/array argument.
pub const ERR_NULL: i32 = -1;
/// Error: a panic was caught at the boundary.
pub const ERR_PANIC: i32 = -2;
/// Error: a size, stride or array length is invalid.
pub const ERR_INVALID: i32 = -3;
/// Error: a `ByteBuffer` argument is not a direct buffer.
pub const ERR_NOT_DIRECT: i32 = -4;
/// Error: a JNI call failed.
pub const ERR_JNI: i32 = -5;

/// Length of the `out` array.
pub const OUT_LEN: usize = 13;
/// `out` index: 1 if locked.
pub const OUT_LOCKED: usize = 0;
/// `out` index: profile (0 S, 1 M, 2 L, -1 unknown).
pub const OUT_PROFILE: usize = 1;
/// `out` index: shape id (-1 unknown).
pub const OUT_SHAPE: usize = 2;
/// `out` index: symbols held.
pub const OUT_HAVE: usize = 3;
/// `out` index: symbols needed.
pub const OUT_NEED: usize = 4;
/// `out` index: segments done.
pub const OUT_SEGMENTS_DONE: usize = 5;
/// `out` index: segments total.
pub const OUT_SEGMENTS_TOTAL: usize = 6;
/// `out` index: 1 if the particles were read.
pub const OUT_LIVENESS_READ: usize = 7;
/// `out` index: 1 if the particles' seed matches the header.
pub const OUT_LIVENESS_CONSISTENT: usize = 8;
/// `out` index: low byte of the seed shown by the particles.
pub const OUT_SEED_LOW: usize = 9;
/// `out` index: rolling value shown by the particles.
pub const OUT_ROLLING: usize = 10;
/// `out` index: number of valid anchors.
pub const OUT_ANCHOR_COUNT: usize = 11;
/// `out` index: hint code.
pub const OUT_HINT: usize = 12;

/// Length of the `outGeom` array.
pub const GEOM_LEN: usize = 9;
/// `outGeom` index of the first anchor coordinate (`x0`); `y0` follows, then `x1, y1, ...`.
pub const GEOM_ANCHORS: usize = 0;
/// `outGeom` index of the sharpness score.
pub const GEOM_SHARPNESS: usize = 8;

/// Hint code: no advice.
pub const HINT_NONE: i32 = 0;
/// Hint code: point the camera at the code.
pub const HINT_POINT_AT_CODE: i32 = 1;
/// Hint code: move back, the whole code must be in view.
pub const HINT_MOVE_BACK: i32 = 2;
/// Hint code: hold the phone steady, facing the code.
pub const HINT_HOLD_STEADY: i32 = 3;
/// Hint code: too dark or too much glare.
pub const HINT_LIGHTING: i32 = 4;
/// Hint code: the code needs a newer version of the app.
pub const HINT_UPDATE_APP: i32 = 5;
/// Hint code: some other advice; read the text with `nativeHint`.
pub const HINT_OTHER: i32 = 6;

/// Map the decoder's user-facing hint text to a stable code, so the UI can localize it.
///
/// Matching is by keyword, so rewording the English text does not silently break the
/// mapping; text that matches nothing becomes [`HINT_OTHER`].
pub fn hint_code(hint: Option<&str>) -> i32 {
    let Some(text) = hint else {
        return HINT_NONE;
    };
    let text = text.to_ascii_lowercase();
    let has = |needle: &str| text.contains(needle);
    if has("newer version") || has("update") {
        HINT_UPDATE_APP
    } else if has("too dark") || has("glare") {
        HINT_LIGHTING
    } else if has("move back") || has("whole code") {
        HINT_MOVE_BACK
    } else if has("steady") {
        HINT_HOLD_STEADY
    } else if has("point the camera") {
        HINT_POINT_AT_CODE
    } else {
        HINT_OTHER
    }
}

/// Transfer progress as the receiver last reported it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    /// Distinct symbols held, summed over segments.
    pub have: u32,
    /// Symbols needed if nothing is lost.
    pub need: u32,
    /// Segments decoded and verified.
    pub segments_done: u32,
    /// Segments in the transfer.
    pub segments_total: u32,
}

impl Counts {
    /// Update from a frame's progress. A rejected frame keeps the old counts; `Done` means
    /// everything is held.
    pub fn update(&mut self, progress: &ProgressDto) {
        match *progress {
            ProgressDto::Rejected { .. } => {}
            ProgressDto::Accepted {
                have,
                need,
                segments_done,
                segments_total,
            } => {
                *self = Counts {
                    have,
                    need,
                    segments_done,
                    segments_total,
                };
            }
            ProgressDto::Done { .. } => {
                self.have = self.need;
                self.segments_done = self.segments_total;
            }
        }
    }
}

/// The result of one camera frame, in the form the JVM receives it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameReport {
    /// `STATUS_*`.
    pub status: i32,
    /// The `out` array.
    pub ints: [i32; OUT_LEN],
    /// The `outGeom` array.
    pub geom: [f32; GEOM_LEN],
}

fn to_i32(v: u32) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

/// Pack a frame result and the receiver's running `counts` into the array layout.
pub fn pack(result: &FrameResultDto, counts: Counts) -> FrameReport {
    let status = match result.progress {
        ProgressDto::Rejected { .. } => STATUS_REJECTED,
        ProgressDto::Accepted { .. } => STATUS_ACCEPTED,
        ProgressDto::Done { .. } => STATUS_DONE,
    };
    let mut ints = [0i32; OUT_LEN];
    ints[OUT_LOCKED] = i32::from(result.locked);
    ints[OUT_PROFILE] = result.profile.map_or(-1, |p| match p {
        ProfileDto::S => 0,
        ProfileDto::M => 1,
        ProfileDto::L => 2,
    });
    ints[OUT_SHAPE] = result.shape_id.map_or(-1, i32::from);
    ints[OUT_HAVE] = to_i32(counts.have);
    ints[OUT_NEED] = to_i32(counts.need);
    ints[OUT_SEGMENTS_DONE] = to_i32(counts.segments_done);
    ints[OUT_SEGMENTS_TOTAL] = to_i32(counts.segments_total);
    if let LivenessDto::Read {
        seed_low,
        rolling,
        consistent,
    } = result.liveness
    {
        ints[OUT_LIVENESS_READ] = 1;
        ints[OUT_LIVENESS_CONSISTENT] = i32::from(consistent);
        ints[OUT_SEED_LOW] = i32::from(seed_low);
        ints[OUT_ROLLING] = i32::from(rolling);
    }
    ints[OUT_HINT] = hint_code(result.hint.as_deref());

    let mut geom = [0f32; GEOM_LEN];
    let slots = geom[GEOM_ANCHORS..GEOM_SHARPNESS].chunks_exact_mut(2);
    let mut anchors = 0;
    for (slot, p) in slots.zip(&result.anchors) {
        slot[0] = p.x as f32;
        slot[1] = p.y as f32;
        anchors += 1;
    }
    ints[OUT_ANCHOR_COUNT] = anchors;
    geom[GEOM_SHARPNESS] = result.sharpness as f32;

    FrameReport { status, ints, geom }
}

#[cfg(test)]
mod tests {
    use super::*;
    use particlecode_ffi::api::dto::ImagePointDto;

    fn result(progress: ProgressDto) -> FrameResultDto {
        FrameResultDto {
            locked: true,
            profile: Some(ProfileDto::L),
            shape_id: Some(7),
            anchors: vec![
                ImagePointDto { x: 10.5, y: 20.25 },
                ImagePointDto { x: 30.0, y: 40.0 },
                ImagePointDto { x: 50.0, y: 60.0 },
                ImagePointDto { x: 70.0, y: 80.0 },
            ],
            sharpness: 12.5,
            liveness: LivenessDto::Read {
                seed_low: 0xAB,
                rolling: 0xBEEF,
                consistent: true,
            },
            progress,
            reject_reason: None,
            hint: None,
        }
    }

    #[test]
    fn indices_are_dense_and_distinct() {
        let all = [
            OUT_LOCKED,
            OUT_PROFILE,
            OUT_SHAPE,
            OUT_HAVE,
            OUT_NEED,
            OUT_SEGMENTS_DONE,
            OUT_SEGMENTS_TOTAL,
            OUT_LIVENESS_READ,
            OUT_LIVENESS_CONSISTENT,
            OUT_SEED_LOW,
            OUT_ROLLING,
            OUT_ANCHOR_COUNT,
            OUT_HINT,
        ];
        assert_eq!(all, core::array::from_fn(|i| i));
        assert_eq!(all.len(), OUT_LEN);
        assert_eq!(GEOM_ANCHORS + 8, GEOM_SHARPNESS);
        assert_eq!(GEOM_SHARPNESS + 1, GEOM_LEN);
    }

    #[test]
    fn packs_an_accepted_frame() {
        let progress = ProgressDto::Accepted {
            have: 11,
            need: 20,
            segments_done: 1,
            segments_total: 3,
        };
        let mut counts = Counts::default();
        counts.update(&progress);
        let report = pack(&result(progress), counts);
        assert_eq!(report.status, STATUS_ACCEPTED);
        let o = report.ints;
        assert_eq!(o[OUT_LOCKED], 1);
        assert_eq!(o[OUT_PROFILE], 2);
        assert_eq!(o[OUT_SHAPE], 7);
        assert_eq!(
            [
                o[OUT_HAVE],
                o[OUT_NEED],
                o[OUT_SEGMENTS_DONE],
                o[OUT_SEGMENTS_TOTAL]
            ],
            [11, 20, 1, 3]
        );
        assert_eq!(o[OUT_LIVENESS_READ], 1);
        assert_eq!(o[OUT_LIVENESS_CONSISTENT], 1);
        assert_eq!(o[OUT_SEED_LOW], 0xAB);
        assert_eq!(o[OUT_ROLLING], 0xBEEF);
        assert_eq!(o[OUT_ANCHOR_COUNT], 4);
        assert_eq!(o[OUT_HINT], HINT_NONE);
        assert_eq!(report.geom[0..4], [10.5, 20.25, 30.0, 40.0]);
        assert_eq!(report.geom[6..8], [70.0, 80.0]);
        assert!((report.geom[GEOM_SHARPNESS] - 12.5).abs() < f32::EPSILON);
    }

    #[test]
    fn unlocked_rejection_has_neutral_fields_and_a_hint() {
        let r = FrameResultDto {
            locked: false,
            profile: None,
            shape_id: None,
            anchors: Vec::new(),
            sharpness: 0.0,
            liveness: LivenessDto::Unknown,
            progress: ProgressDto::Rejected {
                reason: "no lock".into(),
            },
            reject_reason: Some("no lock".into()),
            hint: Some("Point the camera at the code".into()),
        };
        let report = pack(&r, Counts::default());
        assert_eq!(report.status, STATUS_REJECTED);
        assert_eq!(report.ints[OUT_LOCKED], 0);
        assert_eq!(report.ints[OUT_PROFILE], -1);
        assert_eq!(report.ints[OUT_SHAPE], -1);
        assert_eq!(report.ints[OUT_LIVENESS_READ], 0);
        assert_eq!(report.ints[OUT_ANCHOR_COUNT], 0);
        assert_eq!(report.ints[OUT_HINT], HINT_POINT_AT_CODE);
        assert!(report.geom.iter().all(|&v| v.to_bits() == 0));
    }

    #[test]
    fn three_anchors_leave_the_center_slot_zero() {
        let mut r = result(ProgressDto::Done {
            payload: Vec::new(),
        });
        r.anchors.truncate(3);
        let report = pack(&r, Counts::default());
        assert_eq!(report.status, STATUS_DONE);
        assert_eq!(report.ints[OUT_ANCHOR_COUNT], 3);
        assert_eq!(report.geom[6..8], [0.0, 0.0]);
    }

    #[test]
    fn counts_survive_rejections_and_complete_on_done() {
        let mut c = Counts::default();
        c.update(&ProgressDto::Accepted {
            have: 4,
            need: 9,
            segments_done: 0,
            segments_total: 2,
        });
        c.update(&ProgressDto::Rejected { reason: "x".into() });
        assert_eq!(
            c,
            Counts {
                have: 4,
                need: 9,
                segments_done: 0,
                segments_total: 2
            }
        );
        c.update(&ProgressDto::Done { payload: vec![] });
        assert_eq!((c.have, c.segments_done), (9, 2));
    }

    #[test]
    fn huge_counts_saturate() {
        let counts = Counts {
            have: u32::MAX,
            ..Counts::default()
        };
        let report = pack(&result(ProgressDto::Done { payload: vec![] }), counts);
        assert_eq!(report.ints[OUT_HAVE], i32::MAX);
    }

    #[test]
    fn hint_texts_map_to_codes() {
        for (text, code) in [
            ("Point the camera at the code", HINT_POINT_AT_CODE),
            ("Move back: the whole code must be in view", HINT_MOVE_BACK),
            ("Hold the phone steady, facing the code", HINT_HOLD_STEADY),
            (
                "Too dark or too much glare: change the angle or the light",
                HINT_LIGHTING,
            ),
            (
                "This code needs a newer version of the app",
                HINT_UPDATE_APP,
            ),
            ("something else", HINT_OTHER),
        ] {
            assert_eq!(hint_code(Some(text)), code, "{text}");
        }
        assert_eq!(hint_code(None), HINT_NONE);
    }
}
