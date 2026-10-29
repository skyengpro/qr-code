//! [`ShapeTable`]: built-in shapes plus application-defined bitmaps, and `shapes.bin`.

use alloc::string::String;
use alloc::vec::Vec;

use super::{
    DISC, FIRST_CUSTOM, LAST_BUILTIN, SHAPE_TABLE_VERSION, ShapeError, builtin_contains,
    builtin_name,
};
use crate::crc::crc32;
use crate::layout::{DotOrder, Lattice, to_design_box};
use crate::profile::Profile;

const MAGIC: &[u8; 4] = b"PCSH";
const FORMAT_VERSION: u8 = 1;
/// Magic + format version + table version + entry count.
const FILE_HEADER_LEN: usize = 8;
const CRC_LEN: usize = 4;

/// An application-defined shape: one mask per profile (S, M, L).
#[derive(Debug, Clone, PartialEq, Eq)]
struct CustomShape {
    id: u8,
    name: String,
    masks: [Vec<bool>; 3],
}

/// The set of shapes a build knows: the built-ins (ids 0–30) plus custom bitmaps
/// (ids 32–255). Both ends of a transfer must hold the same table version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapeTable {
    version: u8,
    /// Sorted by id.
    custom: Vec<CustomShape>,
}

impl Default for ShapeTable {
    fn default() -> Self {
        ShapeTable::builtin()
    }
}

fn profile_index(profile: Profile) -> usize {
    usize::from(profile.code())
}

fn lattice_len(profile: Profile) -> usize {
    Lattice::new(profile).len()
}

impl ShapeTable {
    /// The built-in table at [`SHAPE_TABLE_VERSION`], without custom shapes.
    pub fn builtin() -> ShapeTable {
        ShapeTable {
            version: SHAPE_TABLE_VERSION,
            custom: Vec::new(),
        }
    }

    /// Table version (`0..=3`), announced in the frame header.
    pub fn version(&self) -> u8 {
        self.version
    }

    /// Set the table version, e.g. after changing the custom shapes. Must fit 2 bits.
    pub fn set_version(&mut self, version: u8) -> Result<(), ShapeError> {
        if version > 3 {
            return Err(ShapeError::BadTableVersion(version));
        }
        self.version = version;
        Ok(())
    }

    fn find(&self, id: u8) -> Option<&CustomShape> {
        self.custom
            .binary_search_by_key(&id, |c| c.id)
            .ok()
            .map(|i| &self.custom[i])
    }

    /// Whether `id` names a shape in this table.
    pub fn contains(&self, id: u8) -> bool {
        builtin_name(id).is_some() || self.find(id).is_some()
    }

    /// Display name of shape `id`: `"disc"`, `"rounded-square"`, `"A"`..`"Z"`, `"heart"`,
    /// `"hexagon"`, `"ring"`, or the custom shape's name.
    pub fn name(&self, id: u8) -> Option<&str> {
        builtin_name(id).or_else(|| self.find(id).map(|c| c.name.as_str()))
    }

    /// All ids in the table, ascending.
    pub fn ids(&self) -> impl Iterator<Item = u8> + '_ {
        (0..=LAST_BUILTIN).chain(self.custom.iter().map(|c| c.id))
    }

    /// Bitmap of shape `id` over `Lattice::new(profile).points()` in canonical order, or
    /// `None` if the id is unknown. Keep-outs are *not* applied here (see
    /// [`ShapeTable::dot_order`]).
    pub fn mask(&self, id: u8, profile: Profile) -> Option<Vec<bool>> {
        self.mask_on(id, &Lattice::new(profile))
    }

    fn mask_on(&self, id: u8, lattice: &Lattice) -> Option<Vec<bool>> {
        let profile = lattice.profile();
        if let Some(c) = self.find(id) {
            return Some(c.masks[profile_index(profile)].clone());
        }
        if id == DISC {
            // Lattice points exactly on the circle (e.g. (12, 12√3) at S) must be decided by
            // the same arithmetic as `Point::norm() <= F`; the design-box round trip can
            // differ in the last bit.
            let f = profile.field_half();
            return Some(lattice.points().iter().map(|p| p.at.norm() <= f).collect());
        }
        // Probe the id once so unknown ids return `None` rather than an empty mask.
        builtin_contains(id, 500.0, 500.0)?;
        Some(
            lattice
                .points()
                .iter()
                .map(|p| {
                    let (x, y) = to_design_box(profile, p.at);
                    builtin_contains(id, x, y).unwrap_or(false)
                })
                .collect(),
        )
    }

    /// Payload dots of shape `id` at `profile` in serpentine symbol order (keep-outs
    /// punched out), or `None` if the id is unknown.
    pub fn dot_order(&self, id: u8, profile: Profile) -> Option<DotOrder> {
        let lattice = Lattice::new(profile);
        let mask = self.mask_on(id, &lattice)?;
        Some(DotOrder::new(&lattice, &mask))
    }

    /// Add a custom shape with masks for S, M and L (each one entry per lattice point).
    /// Ids must be in `32..=255` and not yet used.
    pub fn insert(&mut self, id: u8, name: &str, masks: [Vec<bool>; 3]) -> Result<(), ShapeError> {
        if id < FIRST_CUSTOM {
            return Err(ShapeError::ReservedId(id));
        }
        if name.len() > usize::from(u8::MAX) {
            return Err(ShapeError::NameTooLong(name.len()));
        }
        for profile in Profile::ALL {
            let expected = lattice_len(profile);
            let got = masks[profile_index(profile)].len();
            if got != expected {
                return Err(ShapeError::MaskLength {
                    profile,
                    expected,
                    got,
                });
            }
        }
        match self.custom.binary_search_by_key(&id, |c| c.id) {
            Ok(_) => Err(ShapeError::DuplicateId(id)),
            Err(at) => {
                self.custom.insert(
                    at,
                    CustomShape {
                        id,
                        name: String::from(name),
                        masks,
                    },
                );
                Ok(())
            }
        }
    }

    /// Serialize the custom shapes and the table version as `shapes.bin` (see the module
    /// documentation of [`shapes`](crate::shapes)).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.push(FORMAT_VERSION);
        out.push(self.version);
        // At most 224 custom ids exist, so the count always fits.
        out.extend_from_slice(&(self.custom.len() as u16).to_le_bytes());
        for c in &self.custom {
            out.push(c.id);
            out.push(c.name.len() as u8); // checked ≤ 255 in `insert`
            out.extend_from_slice(c.name.as_bytes());
            for mask in &c.masks {
                pack_mask(mask, &mut out);
            }
        }
        let crc = crc32(&out);
        out.extend_from_slice(&crc.to_le_bytes());
        out
    }

    /// Parse a `shapes.bin`. Never panics: malformed input yields a [`ShapeError`].
    pub fn from_bytes(bytes: &[u8]) -> Result<ShapeTable, ShapeError> {
        if bytes.len() < MAGIC.len() || &bytes[..MAGIC.len()] != MAGIC {
            return Err(ShapeError::BadMagic);
        }
        if bytes.len() < FILE_HEADER_LEN + CRC_LEN {
            return Err(ShapeError::Truncated);
        }
        let (body, crc_bytes) = bytes.split_at(bytes.len() - CRC_LEN);
        let format = body[4];
        if format != FORMAT_VERSION {
            return Err(ShapeError::UnsupportedFormat(format));
        }
        let stored = u32::from_le_bytes([crc_bytes[0], crc_bytes[1], crc_bytes[2], crc_bytes[3]]);
        if crc32(body) != stored {
            return Err(ShapeError::BadChecksum);
        }
        let version = body[5];
        if version > 3 {
            return Err(ShapeError::BadTableVersion(version));
        }
        let count = usize::from(u16::from_le_bytes([body[6], body[7]]));

        let lens = Profile::ALL.map(lattice_len);
        let mut reader = Reader {
            rest: &body[FILE_HEADER_LEN..],
        };
        let mut custom: Vec<CustomShape> = Vec::new();
        for _ in 0..count {
            let id = reader.u8()?;
            if id < FIRST_CUSTOM {
                return Err(ShapeError::ReservedId(id));
            }
            if custom.last().is_some_and(|prev| prev.id >= id) {
                return Err(ShapeError::Unsorted);
            }
            let name_len = usize::from(reader.u8()?);
            let name = core::str::from_utf8(reader.take(name_len)?)
                .map_err(|_| ShapeError::BadName)?
                .into();
            let mut masks: [Vec<bool>; 3] = [Vec::new(), Vec::new(), Vec::new()];
            for (mask, &len) in masks.iter_mut().zip(&lens) {
                *mask = unpack_mask(reader.take(len.div_ceil(8))?, len)?;
            }
            custom.push(CustomShape { id, name, masks });
        }
        if !reader.rest.is_empty() {
            return Err(ShapeError::TrailingBytes);
        }
        Ok(ShapeTable { version, custom })
    }
}

fn pack_mask(mask: &[bool], out: &mut Vec<u8>) {
    for chunk in mask.chunks(8) {
        let byte = chunk
            .iter()
            .enumerate()
            .fold(0u8, |acc, (i, &b)| acc | (u8::from(b) << i));
        out.push(byte);
    }
}

fn unpack_mask(bytes: &[u8], len: usize) -> Result<Vec<bool>, ShapeError> {
    let mut mask = Vec::with_capacity(len);
    for (k, &byte) in bytes.iter().enumerate() {
        for bit in 0..8 {
            let on = byte >> bit & 1 == 1;
            if k * 8 + bit < len {
                mask.push(on);
            } else if on {
                return Err(ShapeError::BadPadding);
            }
        }
    }
    Ok(mask)
}

/// Bounds-checked cursor over the entry bytes.
struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ShapeError> {
        if self.rest.len() < n {
            return Err(ShapeError::Truncated);
        }
        let (head, tail) = self.rest.split_at(n);
        self.rest = tail;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8, ShapeError> {
        Ok(self.take(1)?[0])
    }
}
