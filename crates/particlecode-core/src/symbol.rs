//! 3-bit dot symbols.

/// One dot's worth of data: 2 bits of color, 1 bit of size, plus the classifier's
/// confidence. The encoder always emits confidence 255.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Sym3 {
    /// Symbol value `0..8`: `color << 1 | size`.
    pub value: u8,
    /// Classifier confidence `0..=255`. Symbols below the receiver's erasure threshold are
    /// treated as Reed-Solomon erasures.
    pub confidence: u8,
}

impl Sym3 {
    /// A certain symbol (what the encoder produces).
    pub const fn new(value: u8) -> Sym3 {
        Sym3 { value: value & 7, confidence: 255 }
    }

    /// Build from a color index (`0..4`) and size bit (0 small, 1 large).
    pub const fn from_parts(color: u8, size: u8) -> Sym3 {
        Sym3::new(((color & 3) << 1) | (size & 1))
    }

    /// Color index `0..4`.
    pub const fn color(self) -> u8 {
        (self.value >> 1) & 3
    }

    /// Size bit: 0 small, 1 large.
    pub const fn size(self) -> u8 {
        self.value & 1
    }

    /// Same value with a given confidence.
    pub const fn with_confidence(self, confidence: u8) -> Sym3 {
        Sym3 { value: self.value, confidence }
    }
}

/// Pack 3-bit symbols MSB-first into bytes. Trailing bits that do not fill a byte are dropped.
pub fn pack(symbols: &[u8]) -> alloc::vec::Vec<u8> {
    let mut out = alloc::vec::Vec::with_capacity(symbols.len() * 3 / 8);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for &s in symbols {
        acc = (acc << 3) | u32::from(s & 7);
        bits += 3;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// Unpack bytes MSB-first into `count` 3-bit symbols. Missing trailing bits are zero.
pub fn unpack(bytes: &[u8], count: usize) -> alloc::vec::Vec<u8> {
    let mut out = alloc::vec::Vec::with_capacity(count);
    for i in 0..count {
        let bit = i * 3;
        let mut v = 0u8;
        for b in bit..bit + 3 {
            let byte = bytes.get(b / 8).copied().unwrap_or(0);
            v = (v << 1) | ((byte >> (7 - (b % 8))) & 1);
        }
        out.push(v);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_roundtrip() {
        let bytes: alloc::vec::Vec<u8> = (0..=255u8).collect();
        let n = bytes.len() * 8 / 3;
        let syms = unpack(&bytes, n);
        let back = pack(&syms);
        assert_eq!(&back[..], &bytes[..back.len()]);
        assert_eq!(back.len(), n * 3 / 8);
    }

    #[test]
    fn parts() {
        let s = Sym3::from_parts(2, 1);
        assert_eq!(s.value, 5);
        assert_eq!((s.color(), s.size()), (2, 1));
    }
}
