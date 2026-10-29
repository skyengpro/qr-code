//! Shape masks over the hex lattice.
//!
//! STUB — implemented by the shapes work package. Required public surface:
//!
//! ```ignore
//! pub const DISC: u8 = 0; pub const ROUNDED_SQUARE: u8 = 1; // 2..=27 letters A..Z
//! pub const HEART: u8 = 28; pub const HEXAGON: u8 = 29; pub const RING: u8 = 30;
//! pub const fn letter(c: char) -> Option<u8>;
//! pub const SHAPE_TABLE_VERSION: u8; // 2 bits, goes into the header
//! pub struct ShapeTable { .. }
//! impl ShapeTable {
//!     pub fn builtin() -> ShapeTable;
//!     pub fn version(&self) -> u8;
//!     pub fn contains(&self, id: u8) -> bool;
//!     pub fn name(&self, id: u8) -> Option<&str>;
//!     pub fn ids(&self) -> impl Iterator<Item = u8>;
//!     /// Bitmap over `Lattice::new(profile).points()`.
//!     pub fn mask(&self, id: u8, profile: Profile) -> Option<Vec<bool>>;
//!     pub fn dot_order(&self, id: u8, profile: Profile) -> Option<DotOrder>;
//!     pub fn insert(&mut self, id: u8, name: &str, masks: [Vec<bool>; 3]) -> Result<(), ShapeError>;
//!     pub fn from_bytes(bytes: &[u8]) -> Result<ShapeTable, ShapeError>; // shapes.bin
//!     pub fn to_bytes(&self) -> Vec<u8>;
//! }
//! ```
