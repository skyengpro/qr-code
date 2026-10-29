//! Error type of the vision stage.

use particlecode_core::DecodeError;

/// Why a frame could not be read. Every variant means "skip this frame"; the next camera
/// frame arrives in a few tens of milliseconds.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VisionError {
    /// No Particle Code anchors were found, or the image is unusable (too small,
    /// inconsistent planes).
    #[error("no Particle Code found in the image")]
    NoLock,
    /// The anchors were found but part of the code lies outside the image.
    #[error("the code is cut off by the image border")]
    Truncated,
    /// The fitted homography does not explain the anchors, swatches and header dots
    /// (RMS residual above 0.15 pitch): a false lock or heavy distortion.
    #[error("homography residual too large")]
    Residual,
    /// The four calibration hues overlap: too dark, glare, or a strong color cast.
    #[error("lighting unusable: calibration colors overlap")]
    Lighting,
    /// The header could not be decoded.
    #[error("header: {0}")]
    Header(DecodeError),
    /// The header's profile differs from the profile implied by the anchor geometry.
    #[error("header profile does not match the anchor geometry")]
    ProfileMismatch,
    /// The shape id is not in the receiver's table.
    #[error("unknown shape id {0}; update the app")]
    UnknownShape(u8),
}

impl From<DecodeError> for VisionError {
    fn from(e: DecodeError) -> Self {
        VisionError::Header(e)
    }
}
