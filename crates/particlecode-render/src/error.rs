//! Error type of the render crate.

/// Why a render or packaging step failed.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// `size_px` is zero or larger than [`crate::RenderConfig::MAX_SIZE_PX`].
    #[error("image size {0}px is outside 1..={max}", max = crate::RenderConfig::MAX_SIZE_PX)]
    BadSize(u32),
    /// A configuration value is unusable (zero fps, non-finite margin, ...).
    #[error("invalid render configuration: {0}")]
    BadConfig(&'static str),
    /// A packager that needs at least one frame got none.
    #[error("no frames to render")]
    NoFrames,
    /// The manifest and the frame list disagree.
    #[error("manifest does not match the frames: {0}")]
    ManifestMismatch(String),
    /// PNG encoding failed.
    #[error("PNG encoding failed: {0}")]
    Png(#[from] png::EncodingError),
    /// GIF encoding failed.
    #[error("GIF encoding failed: {0}")]
    Gif(#[from] gif::EncodingError),
    /// Manifest serialisation failed.
    #[error("manifest serialisation failed: {0}")]
    Json(#[from] serde_json::Error),
    /// Filesystem error while writing a sequence.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
