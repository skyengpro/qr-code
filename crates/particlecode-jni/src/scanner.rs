//! One decoder session plus the state the JVM polls between frames. Safe code only.

use particlecode_ffi::api::decoder::ParticleDecoder;
use particlecode_ffi::api::dto::{FrameResultDto, ProgressDto};
use particlecode_ffi::native;

use crate::layout::{Counts, FrameReport, pack};

/// What `NativeBridge` keeps behind a handle.
#[derive(Debug, Default)]
pub struct Scanner {
    decoder: ParticleDecoder,
    counts: Counts,
    reject_reason: Option<String>,
    hint: Option<String>,
    payload: Option<Vec<u8>>,
}

impl Scanner {
    /// A scanner that has not seen a frame yet.
    pub fn new() -> Scanner {
        Scanner::default()
    }

    /// Forget the transfer and start over.
    pub fn reset(&mut self) {
        *self = Scanner::default();
    }

    /// Feed one camera frame, borrowing the planes. `None` if the buffers do not fit the
    /// dimensions and strides (see [`native::feed_yuv`]).
    pub fn feed_yuv(
        &mut self,
        width: u32,
        height: u32,
        planes: (&[u8], &[u8], &[u8]),
        strides: (u32, u32, u32),
    ) -> Option<FrameReport> {
        let result = native::feed_yuv(&mut self.decoder, width, height, planes, strides)?;
        Some(self.absorb(result))
    }

    /// Record a frame's outcome and pack it for the arrays.
    fn absorb(&mut self, result: FrameResultDto) -> FrameReport {
        self.counts.update(&result.progress);
        self.reject_reason = match &result.progress {
            ProgressDto::Rejected { reason } => Some(
                result
                    .reject_reason
                    .clone()
                    .unwrap_or_else(|| reason.clone()),
            ),
            _ => None,
        };
        self.hint.clone_from(&result.hint);
        let report = pack(&result, self.counts);
        if let ProgressDto::Done { payload } = result.progress {
            self.payload = Some(payload);
        }
        report
    }

    /// Payload bytes delivered in order since the last call.
    pub fn take_ready(&mut self) -> Option<Vec<u8>> {
        self.decoder.take_ready()
    }

    /// The whole payload, once the transfer is done.
    pub fn payload(&self) -> Option<&[u8]> {
        self.payload.as_deref()
    }

    /// Technical reason the last frame was rejected.
    pub fn reject_reason(&self) -> Option<&str> {
        self.reject_reason.as_deref()
    }

    /// User-facing advice after the last frame.
    pub fn hint(&self) -> Option<&str> {
        self.hint.as_deref()
    }

    /// Session id of the transfer being received, once a frame was accepted.
    pub fn session_id(&self) -> Option<u16> {
        self.decoder.session_id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{ERR_INVALID, OUT_HINT, STATUS_REJECTED};

    #[test]
    fn short_buffers_are_not_a_frame() {
        let mut s = Scanner::new();
        let y = vec![0u8; 100];
        assert!(s.feed_yuv(64, 64, (&y, &y, &y), (64, 32, 1)).is_none());
        assert!(s.feed_yuv(0, 0, (&y, &y, &y), (0, 0, 1)).is_none());
        assert_eq!(ERR_INVALID, -3);
    }

    #[test]
    fn a_blank_frame_is_a_rejection_with_a_hint() {
        let mut s = Scanner::new();
        let (w, h) = (96usize, 96usize);
        let y = vec![128u8; w * h];
        let c = vec![128u8; w * h / 4];
        let report = s
            .feed_yuv(
                w as u32,
                h as u32,
                (&y, &c, &c),
                (w as u32, w as u32 / 2, 1),
            )
            .expect("buffers fit");
        assert_eq!(report.status, STATUS_REJECTED);
        assert!(s.reject_reason().is_some());
        assert!(s.hint().is_some());
        assert_ne!(report.ints[OUT_HINT], 0);
        assert!(s.payload().is_none());
        assert_eq!(s.session_id(), None);
        s.reset();
        assert!(s.reject_reason().is_none());
    }
}
