//! The one error type of the API.

use flutter_rust_bridge::frb;

use particlecode_core::EncodeError;

/// Why an FFI call failed. Decoding never fails this way: a bad frame is reported as
/// [`ProgressDto::Rejected`](super::dto::ProgressDto::Rejected).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FfiError {
    /// A configuration value is unusable (wrong key length, unparsable JSON, ...).
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    /// The core could not build the session (payload too large, shape unknown, ...).
    #[error("encode failed: {0}")]
    Encode(String),
    /// An argument of a call is invalid (null pointer, wrong length, ...).
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    /// A panic was caught at the boundary; this is a bug.
    #[error("internal error: {0}")]
    Internal(String),
}

impl FfiError {
    /// Human-readable description, for logs and UI.
    #[frb(sync)]
    pub fn message(&self) -> String {
        self.to_string()
    }
}

impl From<EncodeError> for FfiError {
    fn from(e: EncodeError) -> Self {
        FfiError::Encode(e.to_string())
    }
}
