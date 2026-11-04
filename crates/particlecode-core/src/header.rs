//! The 96-bit frame header and its 48-dot error-correcting code.
//!
//! Bit layout (MSB first, spec table order):
//!
//! | field          | bits |
//! |----------------|------|
//! | magic          | 6    |
//! | version        | 4    |  (high 2 bits: format version, low 2 bits: shape table version)
//! | profile        | 2    |
//! | palette        | 1    |
//! | static         | 1    |
//! | shape id       | 8    |
//! | session id     | 16   |
//! | payload length | 24   |
//! | segment size   | 4    |  (`2^n` KiB)
//! | segment index  | 10   |
//! | packet seed    | 12   |
//! | header CRC     | 8    |  (CRC-8 over the first 88 bits = 11 bytes)
//!
//! **Header code.** The spec asks for "RS(48,32) over GF(8)", which does not exist (RS
//! codes over GF(8) are at most 7 symbols long). We use the closest real code with the
//! same footprint: **RS(24,16) over GF(2⁶)**. The 96 bits are cut into 16 six-bit data
//! symbols, 8 parity symbols are appended (systematic), and code symbol `i` occupies
//! header dots `2i` (high 3 bits) and `2i + 1` (low 3 bits). Both dots of a symbol sit on
//! the same halo, so losing a whole halo is exactly 8 erasures — still decodable.

use crate::crc::crc8;
use crate::error::DecodeError;
use crate::profile::{Palette, Profile};
use crate::symbol::Sym3;

/// Magic bits that reject false locks.
pub const MAGIC: u8 = 0b10_1101;
/// Format version (high 2 bits of the version field).
pub const FORMAT_VERSION: u8 = 2;
/// Number of header dots.
pub const HEADER_SYMBOLS: usize = 48;

/// Decoded frame header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Header {
    /// Shape table version (2 bits). Both ends must agree.
    pub shape_table_version: u8,
    /// Lattice profile.
    pub profile: Profile,
    /// Palette.
    pub palette: Palette,
    /// 1 = this frame is the whole payload.
    pub is_static: bool,
    /// Mask to sample.
    pub shape_id: u8,
    /// Random per transfer.
    pub session_id: u16,
    /// Payload length in bytes (24 bits).
    pub payload_len: u32,
    /// Segment size is `2^n` KiB, `n = 0..16`.
    pub segment_kb_log2: u8,
    /// Which segment this packet belongs to (10 bits).
    pub segment_index: u16,
    /// RaptorQ encoding symbol id within the segment (12 bits).
    pub packet_seed: u16,
}

impl Header {
    /// Segment size in bytes.
    pub fn segment_size(&self) -> usize {
        1024usize << self.segment_kb_log2
    }

    /// Bytes handed to the fountain coder: payload ‖ CRC-32(payload).
    pub fn framed_len(&self) -> usize {
        self.payload_len as usize + 4
    }

    /// Number of segments, derived — never sent.
    pub fn segment_count(&self) -> usize {
        self.framed_len().div_ceil(self.segment_size())
    }

    /// Length of segment `index`'s slice of the framed payload.
    pub fn segment_len(&self, index: usize) -> usize {
        let start = index * self.segment_size();
        self.framed_len()
            .saturating_sub(start)
            .min(self.segment_size())
    }

    /// Pack into 12 bytes (96 bits), computing the CRC. Field values are masked to width.
    pub fn to_bytes(&self) -> [u8; 12] {
        let mut w = BitWriter::default();
        w.put(u64::from(MAGIC), 6);
        w.put(
            u64::from((FORMAT_VERSION << 2) | (self.shape_table_version & 3)),
            4,
        );
        w.put(u64::from(self.profile.code()), 2);
        w.put(u64::from(self.palette == Palette::Hues8), 1);
        w.put(u64::from(self.is_static), 1);
        w.put(u64::from(self.shape_id), 8);
        w.put(u64::from(self.session_id), 16);
        w.put(u64::from(self.payload_len), 24);
        w.put(u64::from(self.segment_kb_log2), 4);
        w.put(u64::from(self.segment_index), 10);
        w.put(u64::from(self.packet_seed), 12);
        let mut out = [0u8; 12];
        out[..11].copy_from_slice(&w.bytes()[..11]);
        out[11] = crc8(&out[..11]);
        out
    }

    /// Parse 12 bytes, checking magic, CRC and field validity.
    pub fn from_bytes(bytes: &[u8; 12]) -> Result<Header, DecodeError> {
        let mut r = BitReader { bytes, pos: 0 };
        if r.get(6) as u8 != MAGIC {
            return Err(DecodeError::BadMagic);
        }
        if crc8(&bytes[..11]) != bytes[11] {
            return Err(DecodeError::BadHeaderCrc);
        }
        let version = r.get(4) as u8;
        if version >> 2 != FORMAT_VERSION {
            return Err(DecodeError::UnsupportedVersion(version >> 2));
        }
        let profile = Profile::from_code(r.get(2) as u8).ok_or(DecodeError::BadProfile)?;
        let palette = if r.get(1) == 1 {
            Palette::Hues8
        } else {
            Palette::Hues4
        };
        let header = Header {
            shape_table_version: version & 3,
            profile,
            palette,
            is_static: r.get(1) == 1,
            shape_id: r.get(8) as u8,
            session_id: r.get(16) as u16,
            payload_len: r.get(24) as u32,
            segment_kb_log2: r.get(4) as u8,
            segment_index: r.get(10) as u16,
            packet_seed: r.get(12) as u16,
        };
        if usize::from(header.segment_index) >= header.segment_count() {
            return Err(DecodeError::Inconsistent(
                "segment index beyond segment count",
            ));
        }
        Ok(header)
    }

    /// Encode into 48 header dot symbols (values `0..8`). See the module docs.
    pub fn encode(&self) -> [u8; HEADER_SYMBOLS] {
        crate::rs::header_encode(&self.to_bytes())
    }

    /// Decode 48 header dot symbols (with confidences; low-confidence symbols become
    /// erasures).
    pub fn decode(symbols: &[Sym3]) -> Result<Header, DecodeError> {
        if symbols.len() != HEADER_SYMBOLS {
            return Err(DecodeError::WrongSymbolCount {
                expected: HEADER_SYMBOLS,
                got: symbols.len(),
            });
        }
        let bytes = crate::rs::header_decode(symbols)?;
        Header::from_bytes(&bytes)
    }
}

#[derive(Default)]
struct BitWriter {
    buf: [u8; 12],
    pos: usize,
}

impl BitWriter {
    fn put(&mut self, value: u64, width: usize) {
        for i in (0..width).rev() {
            let bit = ((value >> i) & 1) as u8;
            self.buf[self.pos / 8] |= bit << (7 - self.pos % 8);
            self.pos += 1;
        }
    }

    fn bytes(&self) -> &[u8; 12] {
        &self.buf
    }
}

struct BitReader<'a> {
    bytes: &'a [u8; 12],
    pos: usize,
}

impl BitReader<'_> {
    fn get(&mut self, width: usize) -> u64 {
        let mut v = 0u64;
        for _ in 0..width {
            v = (v << 1) | u64::from((self.bytes[self.pos / 8] >> (7 - self.pos % 8)) & 1);
            self.pos += 1;
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    pub(crate) fn sample() -> Header {
        Header {
            shape_table_version: 0,
            profile: Profile::M,
            palette: Palette::Hues4,
            is_static: false,
            shape_id: 0,
            session_id: 0xBEEF,
            payload_len: 6000,
            segment_kb_log2: 3,
            segment_index: 0,
            packet_seed: 0x123,
        }
    }

    #[test]
    fn bytes_roundtrip() {
        let h = sample();
        assert_eq!(Header::from_bytes(&h.to_bytes()), Ok(h));
    }

    #[test]
    fn crc_catches_flips() {
        let mut b = sample().to_bytes();
        b[5] ^= 0x10;
        assert_eq!(Header::from_bytes(&b), Err(DecodeError::BadHeaderCrc));
    }

    fn confident(dots: &[u8; HEADER_SYMBOLS]) -> Vec<Sym3> {
        dots.iter().map(|&v| Sym3::new(v)).collect()
    }

    #[test]
    fn dots_roundtrip() {
        let h = sample();
        assert_eq!(Header::decode(&confident(&h.encode())), Ok(h));
    }

    #[test]
    fn four_symbol_errors_are_corrected() {
        let h = sample();
        let mut dots = confident(&h.encode());
        // Four different GF(2^6) symbols (dot pairs), one with both dots wrong.
        dots[1].value ^= 5;
        dots[8].value ^= 2;
        dots[9].value ^= 7;
        dots[21].value ^= 1;
        dots[40].value ^= 6;
        assert_eq!(Header::decode(&dots), Ok(h));
    }

    #[test]
    fn a_whole_halo_with_zero_confidence_still_decodes() {
        let h = sample();
        for halo in 0..3 {
            let mut dots = confident(&h.encode());
            for d in &mut dots[halo * 16..halo * 16 + 16] {
                *d = Sym3 {
                    value: (d.value + 3) & 7,
                    confidence: 0,
                };
            }
            assert_eq!(Header::decode(&dots), Ok(h), "halo {halo}");
        }
    }

    #[test]
    fn wrong_length_and_noise_are_rejected() {
        assert!(matches!(
            Header::decode(&[Sym3::new(0); 10]),
            Err(DecodeError::WrongSymbolCount {
                expected: 48,
                got: 10
            })
        ));
        // All-zero dots are a valid RS codeword but not a header: magic mismatch.
        assert_eq!(
            Header::decode(&[Sym3::new(0); HEADER_SYMBOLS]),
            Err(DecodeError::BadMagic)
        );
    }

    #[test]
    fn segments() {
        let h = Header {
            payload_len: 8192,
            ..sample()
        };
        assert_eq!(h.segment_count(), 2);
        assert_eq!(h.segment_len(1), 4);
    }
}
