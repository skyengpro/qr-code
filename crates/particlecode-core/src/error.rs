//! Error types.

/// Why a frame, header or symbol stream could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    /// The header's magic bits do not match: not a Particle Code frame (false lock).
    #[error("header magic mismatch")]
    BadMagic,
    /// Header CRC-8 mismatch.
    #[error("header CRC mismatch")]
    BadHeaderCrc,
    /// Reed-Solomon could not correct the header or a body codeword.
    #[error("too many symbol errors to correct")]
    Uncorrectable,
    /// Format version this build does not understand.
    #[error("unsupported format version {0}")]
    UnsupportedVersion(u8),
    /// Shape table version differs from ours; both apps must ship the same table.
    #[error("shape table version {0} does not match ours; update the app")]
    ShapeTableMismatch(u8),
    /// Profile code 3 is reserved.
    #[error("reserved profile code")]
    BadProfile,
    /// The 8-hue palette is reserved in v1.
    #[error("unsupported palette")]
    UnsupportedPalette,
    /// The shape id is not in our table.
    #[error("unknown shape id {0}; update the app")]
    UnknownShape(u8),
    /// Symbol count does not match the profile + shape.
    #[error("expected {expected} symbols, got {got}")]
    WrongSymbolCount {
        /// Expected count.
        expected: usize,
        /// Actual count.
        got: usize,
    },
    /// Packet belongs to a different session than the one being received.
    #[error("packet from another session")]
    OtherSession,
    /// Header fields are inconsistent (e.g. segment index beyond the segment count).
    #[error("inconsistent header: {0}")]
    Inconsistent(&'static str),
    /// A completed segment failed its CRC-32.
    #[error("segment {0} failed its CRC-32")]
    SegmentCrc(u32),
    /// The reassembled payload failed its CRC-32.
    #[error("payload CRC-32 mismatch")]
    PayloadCrc,
}

/// Why a session could not be built.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    /// Payload length does not fit the 24-bit header field (16 MiB − 1).
    #[error("payload of {0} bytes exceeds the 16 MiB limit")]
    PayloadTooLarge(usize),
    /// More than 1024 segments at this segment size; raise `segment_kb_log2`.
    #[error("payload needs {0} segments, more than 1024; use larger segments")]
    TooManySegments(usize),
    /// A segment needs more than 4096 encoding symbols; use smaller segments or a denser
    /// profile.
    #[error("segment needs {0} symbols per pass, more than the 12-bit packet seed allows")]
    SegmentTooLarge(usize),
    /// Static output: the payload does not fit one frame with the chosen shape.
    #[error("payload of {need} bytes does not fit one frame ({capacity} bytes)")]
    DoesNotFit {
        /// Bytes needed (payload + framing).
        need: usize,
        /// Bytes available.
        capacity: usize,
    },
    /// The 8-hue palette is reserved in v1.
    #[error("unsupported palette")]
    UnsupportedPalette,
    /// The shape id is not in the table.
    #[error("unknown shape id {0}")]
    UnknownShape(u8),
    /// Invalid configuration value.
    #[error("invalid configuration: {0}")]
    InvalidConfig(&'static str),
    /// OS randomness unavailable.
    #[error("random number generator failed")]
    Rng,
}
