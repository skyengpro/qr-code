//! The receiver: symbols in, progress and payload out.

use particlecode_core::{Receiver, Sym3};

use super::dto::ProgressDto;

/// A decoder session. Opaque to the host language; feed it frames until
/// [`ProgressDto::Done`], or until the UI gives up and drops it.
///
/// Phase 1 takes symbols that the host already classified. Phase 2 adds `feed_yuv`, which
/// takes camera planes (see the marked spot below).
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
    pub fn new() -> ParticleDecoder {
        ParticleDecoder {
            rx: Receiver::new(),
        }
    }

    /// Forget the transfer and start over.
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

    // PHASE 2 (vision): `feed_yuv` plugs in here.
    //
    //     pub fn feed_yuv(
    //         &mut self,
    //         width: u32, height: u32,
    //         y: &[u8], u: &[u8], v: &[u8],
    //         y_stride: u32, uv_stride: u32, uv_pixel_stride: u32,
    //     ) -> FrameResultDto
    //
    // It builds the vision crate's YUV frame view from the planes, calls
    // `particlecode_vision::locate` (anchors + homography) and then `read`
    // (header, symbols with confidences, liveness), feeds header and body to
    // `self.rx.feed_decoded(&header, &body)` (or `self.rx.feed(..)` with the header symbols
    // included), and returns the resulting `ProgressDto` together with the lock state and
    // liveness so the UI can show "searching / locked / reading". Keep the ProgressDto
    // contract of `feed_symbols`: a frame that cannot be located or read is a rejection,
    // never an error or a panic.

    /// Payload bytes newly delivered in order since the last call (segment by segment, so
    /// large payloads can be consumed while the transfer is still running). After
    /// [`ProgressDto::Done`] the concatenation of everything returned equals the payload.
    pub fn take_ready(&mut self) -> Option<Vec<u8>> {
        self.rx.take_ready()
    }

    /// Session id of the transfer being received, once a frame was accepted.
    pub fn session_id(&self) -> Option<u16> {
        self.rx.session_id()
    }
}

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
