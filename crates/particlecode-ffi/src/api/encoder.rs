//! The sender: payload bytes in, a deterministic stream of frames out.

use std::time::{SystemTime, UNIX_EPOCH};

use particlecode_core::FrameSpec;
use particlecode_core::session::{Session, random_identity};

use super::dto::{EncoderConfig, EncoderInfo, FrameDto};
use super::error::FfiError;
use crate::json::frame_json;

/// An encoder session. Opaque to the host language; create it once per transfer, then ask
/// for frames by index. Construction does the heavy work (one fountain encoder per
/// segment); the frame calls are cheap.
#[derive(Debug)]
pub struct ParticleEncoder {
    session: Session,
    fps: u16,
    shape_id: u8,
}

impl ParticleEncoder {
    /// Start a session for `payload`.
    ///
    /// Random parts of the identity (session id, 32-byte key) and the liveness clock origin
    /// (the wall clock) are filled in unless `config` fixes them; fixing all three makes
    /// the frames fully deterministic.
    pub fn new(payload: Vec<u8>, config: EncoderConfig) -> Result<ParticleEncoder, FfiError> {
        let key = match config.key.as_deref() {
            Some(k) => Some(<[u8; 32]>::try_from(k).map_err(|_| {
                FfiError::InvalidConfig(format!("key must be 32 bytes, got {}", k.len()))
            })?),
            None => None,
        };
        let (random_id, random_key) = random_identity()?;
        let session = Session::with_identity(
            &payload,
            config.display.into(),
            &config.session_config(),
            config.session_id.unwrap_or(random_id),
            key.unwrap_or(random_key),
            config.clock_origin_unix.unwrap_or_else(now_unix),
        )?;
        let shape_id = session.packets().packet(0).0.shape_id;
        Ok(ParticleEncoder {
            session,
            fps: config.fps.max(1),
            shape_id,
        })
    }

    /// Frame `i`, with the liveness clock derived from the index (second
    /// `clock_origin + i / fps`). Deterministic.
    pub fn frame(&self, i: u32) -> FrameDto {
        FrameDto::from(&self.spec(i))
    }

    /// Frame `i` with the particles showing the liveness value of `unix_seconds`. Use the
    /// real wall clock when displaying live, so a verifier's clock agrees.
    pub fn frame_at(&self, i: u32, unix_seconds: u64) -> FrameDto {
        FrameDto::from(&self.session.frame_at(i, unix_seconds))
    }

    /// [`frame`](Self::frame) as the JSON text of the core `FrameSpec` (see
    /// [`crate::json::frame_json`]).
    pub fn frame_json(&self, i: u32) -> String {
        frame_json(&self.spec(i))
    }

    /// [`frame_at`](Self::frame_at) as JSON.
    pub fn frame_json_at(&self, i: u32, unix_seconds: u64) -> String {
        frame_json(&self.session.frame_at(i, unix_seconds))
    }

    /// Frames in one pass over all segments. Indices beyond it continue with fresh repair
    /// symbols, so the stream never repeats.
    pub fn frames_per_pass(&self) -> u32 {
        self.session.frames_per_pass()
    }

    /// Session id carried by every header.
    pub fn session_id(&self) -> u16 {
        self.session.session_id()
    }

    /// The 32-byte session key (share it with verifiers out of band; see
    /// `liveness_verify`).
    pub fn key(&self) -> Vec<u8> {
        self.session.key().to_vec()
    }

    /// Profile, shape, dot count, block size and segment layout.
    pub fn info(&self) -> EncoderInfo {
        let band = self.session.dot_order().particle_band();
        EncoderInfo {
            profile: self.session.profile().into(),
            shape_id: self.shape_id,
            dot_count: self.session.dot_order().len() as u32,
            block_size: self.session.block_size() as u32,
            segment_count: self.session.packets().segment_count() as u32,
            frames_per_pass: self.session.frames_per_pass(),
            session_id: self.session.session_id(),
            fps: self.fps,
            frame_half_side: band.frame_half_side(),
            quiet_zone: particlecode_core::layout::dims::QUIET_ZONE,
        }
    }

    /// The core frame, for the JSON helpers and tests. Not part of the bridged API.
    pub(crate) fn spec(&self, i: u32) -> FrameSpec {
        self.session.frame(i)
    }
}

/// Current Unix time in seconds (0 if the clock is before the epoch).
fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::dto::{ProfileDto, ShapeChoiceDto};

    fn fixed(payload: &[u8]) -> ParticleEncoder {
        let config = EncoderConfig {
            session_id: Some(0xBEEF),
            key: Some(vec![9; 32]),
            clock_origin_unix: Some(1_700_000_000),
            ..EncoderConfig::default()
        };
        ParticleEncoder::new(payload.to_vec(), config).unwrap()
    }

    #[test]
    fn handles_are_send_and_sync() {
        fn check<T: Send + Sync>() {}
        check::<ParticleEncoder>();
    }

    #[test]
    fn fixed_identity_is_deterministic() {
        let a = fixed(b"payload");
        let b = fixed(b"payload");
        assert_eq!(a.frame(3), b.frame(3));
        assert_eq!(a.session_id(), 0xBEEF);
        assert_eq!(a.key(), vec![9; 32]);
        assert_ne!(a.frame(0), a.frame(1));
    }

    #[test]
    fn frame_dto_is_consistent_with_info() {
        let enc = fixed(&[0xAB; 3000]);
        let info = enc.info();
        let f = enc.frame(0);
        assert_eq!(info.profile, ProfileDto::M);
        assert_eq!(f.dots.xs.len(), info.dot_count as usize);
        assert_eq!(f.dots.ys.len(), f.dots.xs.len());
        assert_eq!(f.dots.colors.len(), f.dots.xs.len());
        assert_eq!(f.dots.sizes.len(), f.dots.xs.len());
        assert!(f.dots.colors.iter().all(|&c| c < 4));
        assert!(f.dots.sizes.iter().all(|&s| s < 2));
        assert_eq!(f.header_dots.xs.len(), 48);
        assert_eq!(f.swatches.len(), 48);
        assert_eq!(f.anchors.len(), 4);
        assert_eq!(f.particles.len(), 6);
        assert_eq!(f.session_id, 0xBEEF);
        assert!((f.frame_half_side - info.frame_half_side).abs() < 1e-12);
        assert!(f.particle_band_inner < f.particle_band_outer);
        assert!(f.particle_band_outer < f.frame_half_side);
        assert!((f.view_half_side - f.frame_half_side - f.quiet_zone).abs() < 1e-12);
        assert!(info.frames_per_pass >= 1);
        assert_eq!(enc.frames_per_pass(), info.frames_per_pass);
    }

    #[test]
    fn frame_at_changes_only_the_particles() {
        let enc = fixed(b"clock");
        let a = enc.frame_at(0, 1_700_000_000);
        let b = enc.frame_at(0, 1_700_000_077);
        assert_eq!(a.dots, b.dots);
        assert_eq!(a.header_dots, b.header_dots);
        assert_eq!(enc.frame(0), a);
        // 24 liveness bits; the low byte of the seed is shared, the rolling part differs
        // (a 16-bit collision for this fixed key and these seconds would be a coincidence).
        assert_ne!(a.particles, b.particles);
    }

    #[test]
    fn bad_config_is_an_error_not_a_panic() {
        let bad_key = EncoderConfig {
            key: Some(vec![1; 5]),
            ..EncoderConfig::default()
        };
        assert!(matches!(
            ParticleEncoder::new(vec![1], bad_key),
            Err(FfiError::InvalidConfig(_))
        ));
        let bad_shape = EncoderConfig {
            shape: ShapeChoiceDto::Require { id: 200 },
            ..EncoderConfig::default()
        };
        let err = ParticleEncoder::new(vec![1], bad_shape).unwrap_err();
        assert!(matches!(err, FfiError::Encode(_)));
        assert!(err.message().contains("shape"));
        let big = EncoderConfig {
            is_static: true,
            ..EncoderConfig::default()
        };
        assert!(ParticleEncoder::new(vec![0; 100_000], big).is_err());
    }

    #[test]
    fn random_identity_when_unset() {
        let a = ParticleEncoder::new(b"x".to_vec(), EncoderConfig::default()).unwrap();
        assert_eq!(a.key().len(), 32);
    }
}
