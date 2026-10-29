//! [`SessionSpec`]: the knobs shared by `encode`, `info`, `roundtrip` and the examples,
//! turned into a core [`Session`].

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use particlecode_core::session::{DisplayInfo, random_identity};
use particlecode_core::{Profile, Session, SessionConfig, ShapeChoice};

/// How to build a [`Session`] for a payload.
#[derive(Debug, Clone)]
pub struct SessionSpec {
    /// Lattice profile.
    pub profile: Profile,
    /// Which mask to use.
    pub shape: ShapeChoice,
    /// One self-contained high-ECC frame.
    pub is_static: bool,
    /// Segment size is `1024 << segment_kb_log2` bytes.
    pub segment_kb_log2: u8,
    /// Data frames per second (also drives the liveness clock).
    pub fps: u16,
    /// Session id; random when `None`.
    pub session_id: Option<u16>,
    /// Session key; random when `None`.
    pub key: Option<[u8; 32]>,
    /// Unix second of frame 0; the current time when `None`.
    pub clock_origin: Option<u64>,
}

impl Default for SessionSpec {
    fn default() -> Self {
        SessionSpec {
            profile: Profile::M,
            shape: ShapeChoice::Require(0),
            is_static: false,
            segment_kb_log2: 3,
            fps: 10,
            session_id: None,
            key: None,
            clock_origin: None,
        }
    }
}

impl SessionSpec {
    /// Build the session for `payload`.
    pub fn build(&self, payload: &[u8]) -> Result<Session> {
        let config = SessionConfig {
            segment_kb_log2: self.segment_kb_log2,
            profile: Some(self.profile),
            fps: self.fps,
            is_static: self.is_static,
            shape: self.shape.clone(),
            ..SessionConfig::default()
        };
        let (random_id, random_key) = random_identity().context("no OS randomness available")?;
        let origin = self.clock_origin.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
        });
        // The profile is fixed, so the display only has to exist.
        let display = DisplayInfo {
            width_px: 1080,
            height_px: 1080,
            width_mm: 60.0,
            height_mm: 60.0,
        };
        Session::with_identity(
            payload,
            display,
            &config,
            self.session_id.unwrap_or(random_id),
            self.key.unwrap_or(random_key),
            origin,
        )
        .context("cannot start an encoding session")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_honoured() {
        let spec = SessionSpec {
            session_id: Some(0x1234),
            key: Some([7; 32]),
            clock_origin: Some(1_700_000_000),
            ..SessionSpec::default()
        };
        let a = spec.build(b"hello").expect("session");
        assert_eq!(a.session_id(), 0x1234);
        assert_eq!(a.key(), &[7; 32]);
        let b = spec.build(b"hello").expect("session");
        assert_eq!(a.frame(0), b.frame(0));
    }

    #[test]
    fn unknown_shape_is_an_error() {
        let spec = SessionSpec {
            shape: ShapeChoice::Require(200),
            ..SessionSpec::default()
        };
        assert!(spec.build(b"x").is_err());
    }

    #[test]
    fn static_payload_that_is_too_big_is_an_error() {
        let spec = SessionSpec {
            is_static: true,
            profile: Profile::S,
            ..SessionSpec::default()
        };
        assert!(spec.build(&[0u8; 5000]).is_err());
        assert!(spec.build(&[0u8; 100]).is_ok());
    }
}
