//! Shape masks over the hex lattice.
//!
//! A shape decides which lattice points carry payload dots. Ids 0–30 are **built in**:
//! they are defined analytically as inside-tests in the 1000 × 1000 design box (y down;
//! see [`layout::to_design_box`](crate::layout::to_design_box)), so every app computes
//! identical masks without shipping bitmaps. Ids 32–255 are **custom**: the application
//! supplies one bitmap per profile (normally loaded from `shapes.bin`). Id 31 is reserved.
//!
//! | id | shape |
//! | --- | --- |
//! | 0 | [`DISC`] (radius 500, boundary included) |
//! | 1 | [`ROUNDED_SQUARE`] |
//! | 2–27 | letters `A`–`Z` in a heavy geometric sans, see [`letter`] |
//! | 28 | [`HEART`] |
//! | 29 | [`HEXAGON`] (pointy top and bottom, so the hero anchors sit near its vertices) |
//! | 30 | [`RING`] |
//! | 31 | reserved |
//!
//! Changing any built-in shape changes the dots a receiver samples, so it requires a bump of
//! [`SHAPE_TABLE_VERSION`].
//!
//! # `shapes.bin`
//!
//! The serialized form of the custom entries of a [`ShapeTable`] (built-ins are never
//! stored). All integers are little-endian.
//!
//! ```text
//! offset  size  field
//! 0       4     magic            b"PCSH"
//! 4       1     format version   1
//! 5       1     table version    0..=3 (goes into the frame header)
//! 6       2     entry count      u16
//! 8       ...   entries, in strictly ascending id order
//! end-4   4     CRC-32 (IEEE, as in zlib) of every preceding byte
//!
//! entry:
//!         1     id               32..=255
//!         1     name length n    bytes
//!         n     name             UTF-8
//!         ...   mask for profile S, then M, then L
//!
//! mask:   ceil(len / 8) bytes, where len is the number of points of the profile's
//!         `Lattice` (canonical order). Point k is bit (k % 8) of byte k / 8 (LSB first);
//!         1 means ink. Padding bits of the last byte must be zero.
//! ```
//!
//! [`ShapeTable::from_bytes`] rejects anything else, including trailing bytes.

mod figures;
mod letters;
mod prim;
mod table;

pub use table::ShapeTable;

use crate::profile::Profile;

/// Version of the built-in shape table, carried in the 2 low bits of the header `version`
/// field. Bump it whenever a built-in shape changes.
pub const SHAPE_TABLE_VERSION: u8 = 0;

/// Id of the disc, the default shape.
pub const DISC: u8 = 0;
/// Id of the rounded square.
pub const ROUNDED_SQUARE: u8 = 1;
/// Id of the heart.
pub const HEART: u8 = 28;
/// Id of the hexagon.
pub const HEXAGON: u8 = 29;
/// Id of the ring.
pub const RING: u8 = 30;
/// Id of letter `A`; `B` is `LETTER_BASE + 1` and so on up to `Z` = 27.
pub const LETTER_BASE: u8 = 2;
/// Highest built-in id.
pub const LAST_BUILTIN: u8 = RING;
/// First id applications may define for themselves.
pub const FIRST_CUSTOM: u8 = 32;

/// Shape id of an ASCII letter (`'A'..='Z'`, lowercase accepted), or `None`.
pub const fn letter(c: char) -> Option<u8> {
    match c {
        'A'..='Z' => Some(LETTER_BASE + (c as u8 - b'A')),
        'a'..='z' => Some(LETTER_BASE + (c as u8 - b'a')),
        _ => None,
    }
}

const LETTER_NAMES: [&str; 26] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z",
];

/// Name of a built-in shape.
pub(crate) fn builtin_name(id: u8) -> Option<&'static str> {
    match id {
        DISC => Some("disc"),
        ROUNDED_SQUARE => Some("rounded-square"),
        2..=27 => Some(LETTER_NAMES[usize::from(id - LETTER_BASE)]),
        HEART => Some("heart"),
        HEXAGON => Some("hexagon"),
        RING => Some("ring"),
        _ => None,
    }
}

/// Whether the design-box point `(x, y)` (`0..=1000` on both axes, y down) is inside
/// built-in shape `id`; `None` if `id` is not a built-in shape.
pub fn builtin_contains(id: u8, x: f64, y: f64) -> Option<bool> {
    Some(match id {
        DISC => figures::disc(x, y),
        ROUNDED_SQUARE => figures::rounded_square(x, y),
        LETTER_BASE..=27 => {
            let prims = letters::letter_prims(usize::from(id - LETTER_BASE))?;
            prim::any_contains(prims, x, y)
        }
        HEART => figures::heart(x, y),
        HEXAGON => figures::hexagon(x, y),
        RING => figures::ring(x, y),
        _ => return None,
    })
}

/// Why a shape table could not be built, extended or parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ShapeError {
    /// Custom shapes must use ids 32..=255; 0..=31 belong to the built-in table.
    #[error("shape id {0} is reserved; custom shapes use 32..=255")]
    ReservedId(u8),
    /// A custom shape with this id is already in the table.
    #[error("shape id {0} is already defined")]
    DuplicateId(u8),
    /// Names are at most 255 bytes of UTF-8.
    #[error("shape name of {0} bytes is longer than 255")]
    NameTooLong(usize),
    /// A mask does not have one entry per lattice point of its profile.
    #[error("mask for profile {profile:?} has {got} entries, the lattice has {expected}")]
    MaskLength {
        /// Profile the mask was supplied for.
        profile: Profile,
        /// Lattice point count.
        expected: usize,
        /// Mask length supplied.
        got: usize,
    },
    /// The table version does not fit the header's 2 bits.
    #[error("table version {0} does not fit in 2 bits")]
    BadTableVersion(u8),
    /// The input does not start with `PCSH`.
    #[error("not a shapes.bin file (bad magic)")]
    BadMagic,
    /// The file format version is not one this build reads.
    #[error("unsupported shapes.bin format version {0}")]
    UnsupportedFormat(u8),
    /// The input ends before the structure it announces.
    #[error("shapes.bin is truncated")]
    Truncated,
    /// Bytes remain after the last entry and checksum.
    #[error("trailing bytes after the last shapes.bin entry")]
    TrailingBytes,
    /// The CRC-32 trailer does not match the content.
    #[error("shapes.bin checksum mismatch")]
    BadChecksum,
    /// An entry name is not valid UTF-8.
    #[error("shape name is not valid UTF-8")]
    BadName,
    /// Entries must be in strictly ascending id order.
    #[error("shapes.bin entries are not in ascending id order")]
    Unsorted,
    /// A mask's unused padding bits are set.
    #[error("mask padding bits are not zero")]
    BadPadding,
}

#[cfg(test)]
mod tests;
