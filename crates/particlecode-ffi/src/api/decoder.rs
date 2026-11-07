//! The receiver: symbols in, progress and payload out.

use std::time::Instant;

use flutter_rust_bridge::frb;

use particlecode_core::{DecodeError, Progress, Receiver, Sym3};
use particlecode_vision::{PlaneRef, VisionError, YuvFrame, YuvRef, locate, read, sharpness};

use super::dto::{
    CameraFrameDto, DecodeTimingsDto, FrameResultDto, HintDto, ImagePointDto, LivenessDto,
    PacketDto, ProgressDto, RejectCategoryDto, RejectStageDto,
};

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

    /// Feed one camera frame **in place**: the planes are read straight from the camera's
    /// memory at the addresses in `frame`, with no copy (pointer handoff; see the plugin's
    /// `PROTOCOL.md`). The bridge runs this on its Rust thread pool, so the Dart `Future`
    /// completes once the frame has been decoded.
    ///
    /// # Contract
    ///
    /// The caller guarantees that each `*_address` points to at least `*_len` readable bytes
    /// that stay valid and unchanged until the returned `Future` completes: the plugin's
    /// native camera code holds the frame (Android `ImageProxy`, iOS locked `CVPixelBuffer`)
    /// until Dart releases it after this call. The decoder checks that no address is null
    /// and that the lengths are consistent with the dimensions and strides before it reads
    /// anything; a frame failing those checks is a rejection, never an error. Nothing is read
    /// outside the stated lengths, and nothing is written.
    pub fn feed_camera_frame(&mut self, frame: CameraFrameDto) -> FrameResultDto {
        let start = Instant::now();
        let planes = [
            (frame.y_address, frame.y_len),
            (frame.u_address, frame.u_len),
            (frame.v_address, frame.v_len),
        ];
        if planes
            .iter()
            .any(|&(address, len)| address == 0 || len == 0 || isize::try_from(len).is_err())
        {
            return FrameResultDto::bad_buffers(Stages::validated(start));
        }
        let [y, u, v] = planes.map(|(address, len)| {
            // SAFETY: non-null and at most isize::MAX bytes (checked above); readable,
            // valid and unchanged for the whole call is the caller's documented contract.
            // The slices do not outlive this call.
            #[allow(unsafe_code)]
            unsafe {
                std::slice::from_raw_parts(address as *const u8, len)
            }
        });
        self.feed_yuv_timed(
            start,
            frame.width,
            frame.height,
            (y, u, v),
            (frame.y_stride, frame.uv_stride, frame.uv_pixel_stride),
        )
    }

    /// Feed one RGBA8 camera frame (`4 * width * height` tightly packed bytes), for desktop
    /// and webcam capture. Alpha is ignored. See [`feed_yuv`](Self::feed_yuv) for the
    /// result.
    pub fn feed_rgba(&mut self, width: u32, height: u32, rgba: Vec<u8>) -> FrameResultDto {
        let start = Instant::now();
        let (w, h) = (width as usize, height as usize);
        let Some(n) = w.checked_mul(h).filter(|&n| n > 0 && n <= rgba.len() / 4) else {
            return FrameResultDto::bad_buffers(Stages::validated(start));
        };
        let rgb: Vec<u8> = rgba[..4 * n]
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        let yuv = YuvFrame::from_rgb(w, h, &rgb);
        self.process(&YuvRef::from(&yuv), Stages::validated(start))
    }

    /// [`feed_yuv`](Self::feed_yuv) over borrowed buffers; `strides` is
    /// `(y_stride, uv_stride, uv_pixel_stride)`. Camera memory is never copied except for
    /// de-interleaving chroma. Unusable buffers are reported as a rejection.
    pub(crate) fn feed_yuv_slices(
        &mut self,
        width: u32,
        height: u32,
        planes: (&[u8], &[u8], &[u8]),
        strides: (u32, u32, u32),
    ) -> FrameResultDto {
        self.feed_yuv_timed(Instant::now(), width, height, planes, strides)
    }

    /// [`feed_yuv_slices`](Self::feed_yuv_slices) with the call's start time, so the total
    /// covers work the caller did before.
    fn feed_yuv_timed(
        &mut self,
        start: Instant,
        width: u32,
        height: u32,
        planes: (&[u8], &[u8], &[u8]),
        strides: (u32, u32, u32),
    ) -> FrameResultDto {
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
        let Some(yuv) = planes.view(&mut scratch_u, &mut scratch_v) else {
            return FrameResultDto::bad_buffers(Stages::validated(start));
        };
        self.process(&yuv, Stages::validated(start))
    }

    /// The shared camera pipeline: locate, read, feed. `stages` already holds the
    /// validation time.
    fn process(&mut self, yuv: &YuvRef<'_>, mut stages: Stages) -> FrameResultDto {
        let mark = Instant::now();
        let lock = locate(yuv.luma());
        stages.locate = Some(micros(mark.elapsed()));
        let Some(lock) = lock else {
            return FrameResultDto::rejected(
                VisionError::NoLock,
                "no code in view",
                RejectStageDto::Locate,
                stages,
            );
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
            ..FrameResultDto::rejected(VisionError::NoLock, "", RejectStageDto::Locate, stages)
        };

        let mark = Instant::now();
        let read_result = read(*yuv, &lock, &|p, s| self.rx.dot_order(p, s));
        stages.read = Some(micros(mark.elapsed()));
        let (header, body, liveness) = match read_result {
            Ok(r) => r,
            Err(e) => {
                return FrameResultDto {
                    progress: ProgressDto::Rejected {
                        reason: e.to_string(),
                    },
                    reject_reason: Some(e.to_string()),
                    reject_stage: Some(RejectStageDto::Read),
                    reject_category: Some(RejectCategoryDto::from(&e)),
                    timings: stages.finish(),
                    ..locked.with_hint(vision_hint(&e))
                };
            }
        };
        let distinct_before = self.rx.packets().distinct_packets();
        let mark = Instant::now();
        let progress = self.rx.feed_decoded(&header, &body);
        stages.receive = Some(micros(mark.elapsed()));
        let packet = PacketDto {
            segment_index: header.segment_index,
            packet_seed: header.packet_seed,
            distinct: self.rx.packets().distinct_packets() > distinct_before,
        };
        let (reject_reason, category, hint) = match &progress {
            Progress::Rejected(e) => (
                Some(e.to_string()),
                Some(RejectCategoryDto::from(e)),
                if matches!(
                    e,
                    DecodeError::ShapeTableMismatch(_) | DecodeError::UnknownShape(_)
                ) {
                    HintDto::UpdateApp
                } else {
                    HintDto::None
                },
            ),
            _ => (None, None, HintDto::None),
        };
        FrameResultDto {
            profile: Some(header.profile.into()),
            shape_id: Some(header.shape_id),
            session_id: Some(header.session_id),
            liveness: liveness.into(),
            progress: progress.into(),
            reject_reason,
            reject_stage: category.map(|_| RejectStageDto::Receiver),
            reject_category: category,
            packet: Some(packet),
            timings: stages.finish(),
            ..locked.with_hint(hint)
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

/// User-facing advice for a vision failure.
fn vision_hint(e: &VisionError) -> HintDto {
    match e {
        VisionError::NoLock => HintDto::PointAtCode,
        VisionError::Truncated => HintDto::MoveBack,
        VisionError::Residual | VisionError::ProfileMismatch | VisionError::Header(_) => {
            HintDto::HoldSteady
        }
        VisionError::Lighting => HintDto::Lighting,
        VisionError::UnknownShape(_) => HintDto::UpdateApp,
    }
}

impl FrameResultDto {
    /// A frame whose buffers do not match the stated dimensions and strides.
    fn bad_buffers(stages: Stages) -> FrameResultDto {
        FrameResultDto {
            reject_category: Some(RejectCategoryDto::InvalidBuffers),
            ..FrameResultDto::rejected(
                VisionError::NoLock,
                "inconsistent image buffers",
                RejectStageDto::Buffers,
                stages,
            )
        }
    }

    /// A frame that was not locked: nothing found, or unusable buffers.
    fn rejected(
        e: VisionError,
        detail: &str,
        stage: RejectStageDto,
        stages: Stages,
    ) -> FrameResultDto {
        let reason = if detail.is_empty() {
            e.to_string()
        } else {
            format!("{e} ({detail})")
        };
        FrameResultDto {
            locked: false,
            profile: None,
            shape_id: None,
            session_id: None,
            anchors: Vec::new(),
            sharpness: 0.0,
            liveness: LivenessDto::Unknown,
            progress: ProgressDto::Rejected {
                reason: reason.clone(),
            },
            reject_reason: Some(reason),
            hint: None,
            hint_code: HintDto::None,
            timings: stages.finish(),
            reject_stage: Some(stage),
            reject_category: Some(RejectCategoryDto::from(&e)),
            packet: None,
        }
        .with_hint(vision_hint(&e))
    }

    /// Sets `hint` and `hint_code` together.
    fn with_hint(self, code: HintDto) -> FrameResultDto {
        FrameResultDto {
            hint: code.text().map(str::to_owned),
            hint_code: code,
            ..self
        }
    }
}

/// Elapsed microseconds of the pipeline stages of one call.
#[derive(Debug, Clone, Copy)]
struct Stages {
    start: Instant,
    validate: Option<u32>,
    locate: Option<u32>,
    read: Option<u32>,
    receive: Option<u32>,
}

impl Stages {
    /// Validation of the input finished now; it took the time since `start`.
    fn validated(start: Instant) -> Stages {
        Stages {
            start,
            validate: Some(micros(start.elapsed())),
            locate: None,
            read: None,
            receive: None,
        }
    }

    fn finish(self) -> DecodeTimingsDto {
        DecodeTimingsDto {
            validate_us: self.validate,
            locate_us: self.locate,
            read_us: self.read,
            receive_us: self.receive,
            total_us: micros(self.start.elapsed()),
        }
    }
}

/// A duration in microseconds, saturating.
fn micros(d: std::time::Duration) -> u32 {
    u32::try_from(d.as_micros()).unwrap_or(u32::MAX)
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
