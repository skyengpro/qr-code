//! The dot-field body: one RaptorQ encoding symbol per frame, whitened, cut into
//! Reed-Solomon codewords over GF(2⁸), interleaved, and mapped to 3-bit dot symbols.
//!
//! STUB — implemented by the codec work package. Required public surface:
//!
//! ```ignore
//! pub struct WhitenKey { pub session_id: u16, pub segment_index: u16, pub packet_seed: u16 }
//! pub struct BodyCodec { .. }
//! impl BodyCodec {
//!     /// Layout for `dots` payload dots at `ecc`.
//!     pub fn new(dots: usize, ecc: EccLevel) -> BodyCodec;
//!     /// Data bytes per frame (= the RaptorQ symbol size T).
//!     pub fn block_size(&self) -> usize;
//!     /// `data.len() == block_size()`; returns exactly `dots` symbols (values 0..8).
//!     pub fn encode(&self, data: &[u8], key: WhitenKey) -> Vec<u8>;
//!     /// `symbols.len() == dots`; low-confidence symbols become byte erasures.
//!     pub fn decode(&self, symbols: &[Sym3], key: WhitenKey) -> Result<Vec<u8>, DecodeError>;
//! }
//! ```
