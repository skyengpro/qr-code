//! Reed-Solomon codes over [`crate::gf`] with errors-and-erasures decoding.
//!
//! STUB — implemented by the codec work package. Required public surface:
//!
//! * `pub struct ReedSolomon` generic over a `&'static Gf`, `n ≤ 2^m − 1`, any parity count
//!   (systematic: data first, then parity; generator roots α^0..α^(parity−1)).
//!   `encode(&self, data: &[u8]) -> Vec<u8>` (parity only or full codeword — document it) and
//!   `decode(&self, codeword: &mut [u8], erasures: &[usize]) -> Result<usize, DecodeError>`
//!   (returns corrected count; `2·errors + erasures ≤ parity`).
//! * `header_encode` / `header_decode` below (RS(24,16) over GF(2⁶), see `header.rs`).

use crate::error::DecodeError;
use crate::header::HEADER_SYMBOLS;
use crate::symbol::Sym3;

/// Encode the 12 header bytes into 48 dot symbols (RS(24,16) over GF(2⁶), symbol `i` →
/// dots `2i` (high 3 bits) and `2i+1` (low 3 bits)).
pub fn header_encode(bytes: &[u8; 12]) -> [u8; HEADER_SYMBOLS] {
    let _ = bytes;
    todo!("codec work package")
}

/// Inverse of [`header_encode`]. Dots with low confidence become erasures of their symbol.
pub fn header_decode(symbols: &[Sym3]) -> Result<[u8; 12], DecodeError> {
    let _ = symbols;
    todo!("codec work package")
}
