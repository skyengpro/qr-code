//! Error type of the shape tool.

use std::fmt;

/// Why a shape could not be read, rasterized or analysed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The SVG path data is malformed or empty.
    BadPath(String),
    /// The `.svg` file has no usable `<path d="...">`.
    BadSvg(String),
    /// A numeric option is out of range.
    BadOption(String),
    /// Stroking the path produced nothing (zero-length path or zero width).
    EmptyOutline,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadPath(why) => write!(f, "bad SVG path data: {why}"),
            Error::BadSvg(why) => write!(f, "bad SVG file: {why}"),
            Error::BadOption(why) => write!(f, "bad option: {why}"),
            Error::EmptyOutline => write!(f, "the path has no area to fill or stroke"),
        }
    }
}

impl std::error::Error for Error {}
