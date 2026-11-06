//! The receiver: symbols in, progress and payload out.

use flutter_rust_bridge::frb;

use particlecode_core::{DecodeError, Progress, Receiver, Sym3};
use particlecode_vision::{PlaneRef, VisionError, YuvFrame, YuvRef, locate, read, sharpness};

use super::dto::{FrameResultDto, ImagePointDto, LivenessDto, ProgressDto};

/// A decoder session. Opaque to the host language; feed it frames until
/// [`ProgressDto::Done`], or until the UI gives up and drops it.
///
/// Feed it either camera images ([`feed_yuv`](Self::feed_yuv),
/// [`feed_rgba`](Self::feed_rgba)) or symbols that the host already classified
/// ([`feed_symbols`](Self::feed_symbols)). All of them drive the same receiver.
#[derive(Debug)]
pub struct ParticleDecoder {
    rx: Receiver,
}

impl Default for ParticleDecoder {
    fn default() -> Self {
        ParticleDecoder::new()
    }
}

impl ParticleDecoder {
    /// A decoder that has not seen any frame yet, with the built-in shape table.
    #[frb(sync)]
    pub fn new() -> ParticleDecoder {
        ParticleDecoder {
            rx: Receiver::new(),
        }
    }

    /// Forget the transfer and start over.
    #[frb(sync)]
    pub fn reset(&mut self) {
        self.rx = Receiver::new();
    }

    /// Feed one frame's symbols: the 48 header dots followed by the body dots of the
    /// frame's profile and shape.
    ///
    /// `symbols[i]` is the 3-bit value `color << 1 | size` (`0..8`). `confidences[i]` is the
    /// classifier's confidence (`0..=255`; low values become Reed-Solomon erasures); pass
    /// an empty vector to mean "certain" (255 everywhere). A length mismatch or a symbol
    /// above 7 rejects the frame.
    pub fn feed_symbols(&mut self, symbols: Vec<u8>, confidences: Vec<u8>) -> ProgressDto {
        if !confidences.is_empty() && confidences.len() != symbols.len() {
            return ProgressDto::rejected_because("confidences and symbols differ in length");
        }
        let mut syms = Vec::with_capacity(symbols.len());
        for (i, &value) in symbols.iter().enumerate() {
            if value > 7 {
                return ProgressDto::rejected_because("symbol value above 7");
            }
            let confidence = confidences.get(i).copied().unwrap_or(255);
            syms.push(Sym3::new(value).with_confidence(confidence));
        }
        self.rx.feed(&syms).into()
    }

    /// Feed one camera frame in YUV 4:2:0 (Android `YUV_420_888`, iOS NV12/NV21, I420).
    ///
    /// * `y` is the luma plane: `height` rows of `width` bytes, `y_stride` bytes apart.
    /// * `u` and `v` are the Cb and Cr planes at half resolution (`ceil(width / 2)` x
    ///   `ceil(height / 2)` samples). Chroma sample `(col, row)` is the byte
    ///   `row * uv_stride + col * uv_pixel_stride` of its buffer, so planar I420 uses
    ///   `uv_pixel_stride = 1` and interleaved chroma uses `2`. For NV21 (Android) pass the
    ///   `V` and `U` buffers as the camera plugin gives them (they alias one interleaved
    ///   buffer); for NV12 pass the interleaved plane as `u` and the same plane from byte 1
    ///   as `v`.
    ///
    /// Locates the code, reads the frame, and feeds it to the receiver. A frame that
    /// cannot be located or read (including inconsistent buffers) is a rejection described
    /// in the result, never an error.
    ///
    /// The generated bindings copy the byte vectors once; call this on a worker isolate.
    #[allow(clippy::too_many_arguments)] // flat on purpose: the bridge cannot take a struct of borrows
    pub fn feed_yuv(
        &mut self,
        width: u32,
        height: u32,
        y: Vec<u8>,
        u: Vec<u8>,
        v: Vec<u8>,
        y_stride: u32,
        uv_stride: u32,
        uv_pixel_stride: u32,
    ) -> FrameResultDto {
        self.feed_yuv_slices(
            width,
            height,
            (&y, &u, &v),
            (y_stride, uv_stride, uv_pixel_stride),
        )
    }

    /// Feed one RGBA8 camera frame (`4 * width * height` tightly packed bytes), for desktop
    /// and webcam capture. Alpha is ignored. See [`feed_yuv`](Self::feed_yuv) for the
    /// result.
    pub fn feed_rgba(&mut self, width: u32, height: u32, rgba: Vec<u8>) -> FrameResultDto {
        self.try_feed_rgba(width, height, &rgba)
            .unwrap_or_else(FrameResultDto::bad_buffers)
    }

    /// [`feed_yuv`](Self::feed_yuv) over borrowed buffers; `strides` is
    /// `(y_stride, uv_stride, uv_pixel_stride)`. Not bridged: it is what the C ABI calls,
    /// so camera memory is never copied except for de-interleaving chroma.
    ///
    /// `None` if the buffers are inconsistent with the dimensions and strides.
    pub(crate) fn try_feed_yuv(
        &mut self,
        width: u32,
        height: u32,
        planes: (&[u8], &[u8], &[u8]),
        strides: (u32, u32, u32),
    ) -> Option<FrameResultDto> {
        let (mut scratch_u, mut scratch_v) = (Vec::new(), Vec::new());
        let planes = YuvPlanes {
            width: width as usize,
            height: height as usize,
            y: planes.0,
            u: planes.1,
            v: planes.2,
            y_stride: strides.0 as usize,
            uv_stride: strides.1 as usize,
            uv_pixel_stride: strides.2 as usize,
        };
        let yuv = planes.view(&mut scratch_u, &mut scratch_v)?;
        Some(self.process(&yuv))
    }

    /// [`try_feed_yuv`](Self::try_feed_yuv) with unusable buffers reported as a rejection.
    pub(crate) fn feed_yuv_slices(
        &mut self,
        width: u32,
        height: u32,
        planes: (&[u8], &[u8], &[u8]),
        strides: (u32, u32, u32),
    ) -> FrameResultDto {
        self.try_feed_yuv(width, height, planes, strides)
            .unwrap_or_else(FrameResultDto::bad_buffers)
    }

    /// [`feed_rgba`](Self::feed_rgba) over a borrowed buffer. `None` if `rgba` is shorter
    /// than `4 * width * height` or the size is zero.
    pub(crate) fn try_feed_rgba(
        &mut self,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Option<FrameResultDto> {
        let (w, h) = (width as usize, height as usize);
        let n = w.checked_mul(h).filter(|&n| n > 0 && n <= rgba.len() / 4)?;
        let rgb: Vec<u8> = rgba[..4 * n]
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        let yuv = YuvFrame::from_rgb(w, h, &rgb);
        Some(self.process(&YuvRef::from(&yuv)))
    }

    /// The shared camera pipeline: locate, read, feed.
    fn process(&mut self, yuv: &YuvRef<'_>) -> FrameResultDto {
        let Some(lock) = locate(yuv.luma()) else {
            return FrameResultDto::rejected(VisionError::NoLock, "no code in view");
        };
        let sharp = sharpness(yuv.luma(), &lock);
        let mut anchors: Vec<ImagePointDto> = lock
            .anchors_px
            .iter()
            .map(|&(x, y)| ImagePointDto { x, y })
            .collect();
        anchors.extend(lock.center_px.map(|(x, y)| ImagePointDto { x, y }));
        let locked = FrameResultDto {
            locked: true,
            anchors,
            sharpness: sharp,
            ..FrameResultDto::rejected(VisionError::NoLock, "")
        };

        let read_result = read(*yuv, &lock, &|p, s| self.rx.dot_order(p, s));
        let (header, body, liveness) = match read_result {
            Ok(r) => r,
            Err(e) => {
                return FrameResultDto {
                    progress: ProgressDto::Rejected {
                        reason: e.to_string(),
                    },
                    reject_reason: Some(e.to_string()),
                    hint: Some(vision_hint(&e).into()),
                    ..locked
                };
            }
        };
        let progress = self.rx.feed_decoded(&header, &body);
        let (reject_reason, hint) = match &progress {
            Progress::Rejected(e) => (
                Some(e.to_string()),
                matches!(
                    e,
                    DecodeError::ShapeTableMismatch(_) | DecodeError::UnknownShape(_)
                )
                .then(|| UPDATE_HINT.to_owned()),
            ),
            _ => (None, None),
        };
        FrameResultDto {
            profile: Some(header.profile.into()),
            shape_id: Some(header.shape_id),
            liveness: liveness.into(),
            progress: progress.into(),
            reject_reason,
            hint,
            ..locked
        }
    }

    /// Payload bytes newly delivered in order since the last call (segment by segment, so
    /// large payloads can be consumed while the transfer is still running). After
    /// [`ProgressDto::Done`] the concatenation of everything returned equals the payload.
    #[frb(sync)]
    pub fn take_ready(&mut self) -> Option<Vec<u8>> {
        self.rx.take_ready()
    }

    /// Session id of the transfer being received, once a frame was accepted.
    #[frb(sync)]
    pub fn session_id(&self) -> Option<u16> {
        self.rx.session_id()
    }
}

const UPDATE_HINT: &str = "This code needs a newer version of the app";

/// User-facing advice for a vision failure.
fn vision_hint(e: &VisionError) -> &'static str {
    match e {
        VisionError::NoLock => "Point the camera at the code",
        VisionError::Truncated => "Move back: the whole code must be in view",
        VisionError::Residual | VisionError::ProfileMismatch | VisionError::Header(_) => {
            "Hold the phone steady, facing the code"
        }
        VisionError::Lighting => "Too dark or too much glare: change the angle or the light",
        VisionError::UnknownShape(_) => UPDATE_HINT,
    }
}

impl FrameResultDto {
    /// A frame whose buffers do not match the stated dimensions and strides.
    fn bad_buffers() -> FrameResultDto {
        FrameResultDto::rejected(VisionError::NoLock, "inconsistent image buffers")
    }

    /// A frame that was not locked: nothing found, or unusable buffers.
    fn rejected(e: VisionError, detail: &str) -> FrameResultDto {
        let reason = if detail.is_empty() {
            e.to_string()
        } else {
            format!("{e} ({detail})")
        };
        FrameResultDto {
            locked: false,
            profile: None,
            shape_id: None,
            anchors: Vec::new(),
            sharpness: 0.0,
            liveness: LivenessDto::Unknown,
            progress: ProgressDto::Rejected {
                reason: reason.clone(),
            },
            reject_reason: Some(reason),
            hint: Some(vision_hint(&e).into()),
        }
    }
}

/// Borrowed camera planes, before validation.
struct YuvPlanes<'a> {
    width: usize,
    height: usize,
    y: &'a [u8],
    u: &'a [u8],
    v: &'a [u8],
    y_stride: usize,
    uv_stride: usize,
    uv_pixel_stride: usize,
}

impl<'a> YuvPlanes<'a> {
    /// A validated view. Luma and planar chroma are borrowed as they are; interleaved
    /// chroma is de-interleaved into the scratch buffers (a quarter of the luma size).
    fn view(&self, scratch_u: &'a mut Vec<u8>, scratch_v: &'a mut Vec<u8>) -> Option<YuvRef<'a>> {
        let luma = PlaneRef::new(self.width, self.height, self.y_stride, self.y)?;
        let (cw, ch) = (self.width.div_ceil(2), self.height.div_ceil(2));
        let u = self.chroma(self.u, cw, ch, scratch_u)?;
        let v = self.chroma(self.v, cw, ch, scratch_v)?;
        YuvRef::new(luma, u, v)
    }

    fn chroma(
        &self,
        data: &'a [u8],
        cw: usize,
        ch: usize,
        scratch: &'a mut Vec<u8>,
    ) -> Option<PlaneRef<'a>> {
        let step = self.uv_pixel_stride;
        if step == 1 {
            return PlaneRef::new(cw, ch, self.uv_stride, data);
        }
        // Row r holds samples at r * stride + c * step; the last one must exist.
        let last = (ch.checked_sub(1)?)
            .checked_mul(self.uv_stride)?
            .checked_add((cw.checked_sub(1)?).checked_mul(step)?)?;
        if step == 0 || last >= data.len() {
            return None;
        }
        scratch.clear();
        scratch.reserve(cw * ch);
        for row in 0..ch {
            let base = row * self.uv_stride;
            scratch.extend((0..cw).map(|c| data[base + c * step]));
        }
        let packed: &'a [u8] = scratch;
        PlaneRef::new(cw, ch, cw, packed)
    }
}

#[cfg(test)]
mod camera_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::dto::EncoderConfig;
    use crate::api::encoder::ParticleEncoder;

    fn payload(n: usize) -> Vec<u8> {
        let mut x = 0x2468_ACE1u32;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 8) as u8
            })
            .collect()
    }

    /// The symbols a perfect camera would read from `frame`, header first.
    fn symbols_of(enc: &ParticleEncoder, i: u32) -> Vec<u8> {
        let spec = enc.spec(i);
        spec.all_symbols().iter().map(|s| s.value).collect()
    }

    #[test]
    fn encoder_to_decoder_round_trip_with_dropped_frames() {
        let data = payload(5000);
        let enc = ParticleEncoder::new(data.clone(), EncoderConfig::default()).unwrap();
        let mut dec = ParticleDecoder::new();
        assert_eq!(dec.session_id(), None);

        let mut streamed = Vec::new();
        let mut result = None;
        let mut accepted = 0u32;
        for i in 0..enc.frames_per_pass() * 4 {
            // Drop every third frame: the fountain must make up for it.
            if i % 3 == 1 {
                continue;
            }
            let progress = dec.feed_symbols(symbols_of(&enc, i), Vec::new());
            if let Some(bytes) = dec.take_ready() {
                streamed.extend(bytes);
            }
            match progress {
                ProgressDto::Accepted { have, need, .. } => {
                    assert!(have <= need);
                    accepted += 1;
                }
                ProgressDto::Done { payload } => {
                    result = Some(payload);
                    break;
                }
                ProgressDto::Rejected { reason } => panic!("rejected frame {i}: {reason}"),
            }
        }
        assert!(accepted > 0);
        assert_eq!(result.as_deref(), Some(data.as_slice()));
        assert_eq!(streamed, data);
        assert_eq!(dec.session_id(), Some(enc.session_id()));
    }

    #[test]
    fn explicit_confidences_are_accepted() {
        let data = payload(300);
        let enc = ParticleEncoder::new(data.clone(), EncoderConfig::default()).unwrap();
        let mut dec = ParticleDecoder::new();
        let mut done = None;
        for i in 0..enc.frames_per_pass() * 3 {
            let syms = symbols_of(&enc, i);
            let conf = vec![200; syms.len()];
            if let ProgressDto::Done { payload } = dec.feed_symbols(syms, conf) {
                done = Some(payload);
                break;
            }
        }
        assert_eq!(done, Some(data));
    }

    #[test]
    fn mixed_header_and_body_are_rejected_without_losing_progress() {
        use crate::api::dto::{ProfileDto, ShapeChoiceDto};
        let data = payload(6000);
        let enc = ParticleEncoder::new(
            data.clone(),
            EncoderConfig {
                shape: ShapeChoiceDto::Require { id: 30 },
                profile: Some(ProfileDto::S),
                ..EncoderConfig::default()
            },
        )
        .unwrap();
        let mut dec = ParticleDecoder::new();
        assert!(matches!(
            dec.feed_symbols(symbols_of(&enc, 0), Vec::new()),
            ProgressDto::Accepted { have: 1, .. }
        ));
        let mut mixed = symbols_of(&enc, 1);
        let next = symbols_of(&enc, 2);
        mixed[48..].copy_from_slice(&next[48..]);
        assert!(matches!(
            dec.feed_symbols(mixed, Vec::new()),
            ProgressDto::Rejected { .. }
        ));
        for i in 1..enc.frames_per_pass() * 3 {
            let progress = dec.feed_symbols(symbols_of(&enc, i), Vec::new());
            match progress {
                ProgressDto::Done { payload } => {
                    assert_eq!(payload, data);
                    return;
                }
                ProgressDto::Accepted { have, .. } => assert!(have >= 1),
                ProgressDto::Rejected { reason } => panic!("frame {i} rejected: {reason}"),
            }
        }
        panic!("transfer did not complete");
    }

    #[test]
    fn malformed_input_is_rejected_not_panicked() {
        let mut dec = ParticleDecoder::new();
        for (syms, conf) in [
            (vec![], vec![]),
            (vec![1; 10], vec![]),
            (vec![1; 10], vec![255; 3]),
            (vec![8; 48], vec![]),
            (vec![0; 48], vec![]),
            (vec![3; 100_000], vec![]),
        ] {
            assert!(
                matches!(dec.feed_symbols(syms, conf), ProgressDto::Rejected { .. }),
                "garbage must be rejected"
            );
        }
        assert_eq!(dec.take_ready(), None);
    }

    #[test]
    fn reset_forgets_the_session() {
        let enc = ParticleEncoder::new(payload(100), EncoderConfig::default()).unwrap();
        let mut dec = ParticleDecoder::new();
        dec.feed_symbols(symbols_of(&enc, 0), Vec::new());
        assert!(dec.session_id().is_some());
        dec.reset();
        assert_eq!(dec.session_id(), None);
    }
}
