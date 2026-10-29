//! The dot-field body: one `RaptorQ` encoding symbol per frame, whitened, cut into
//! Reed-Solomon codewords over GF(2⁸), interleaved, and mapped to 3-bit dot symbols.
//!
//! Layout for `dots` payload dots (see `docs/design.md`, "Body"):
//!
//! * `B = ⌊3·dots / 8⌋` coded bytes, `C = ⌈B / 255⌉` codewords of near-equal length (the first
//!   `B mod C` are one byte longer). A codeword of length `n` has `⌈n · parity₂₅₅ / 255⌉`
//!   parity bytes.
//! * The block size `T` is the total number of data bytes, a pure function of
//!   `(dots, ecc)`.
//! * The `T` data bytes are XOR-ed with a keystream ([`WhitenKey`]) before RS encoding.
//! * Byte `j` of codeword `c` goes to stream position `j·C + c`; holes left by shorter
//!   codewords are skipped. The stream is cut into 3-bit symbols MSB-first. Spare bits of the
//!   last stream symbol and any leftover dots (at most two) are filled from the keystream so
//!   they look random; the decoder ignores them.

use alloc::vec;
use alloc::vec::Vec;

use crate::error::DecodeError;
use crate::gf::GF256;
use crate::profile::EccLevel;
use crate::rs::{ERASURE_BELOW, ReedSolomon, lowest_confidence};
use crate::symbol::{Sym3, unpack};

/// Longest RS(·, ·) codeword over GF(2⁸).
const MAX_CODEWORD: usize = 255;

/// Seeds the whitening keystream; together with the domain constant it identifies one packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct WhitenKey {
    /// Random per transfer (header `session id`).
    pub session_id: u16,
    /// Segment the packet belongs to (header `segment index`).
    pub segment_index: u16,
    /// Encoding symbol id within the segment (header `packet seed`).
    pub packet_seed: u16,
}

/// Deterministic byte keystream: splitmix64 words, each emitted little-endian.
struct Keystream {
    state: u64,
    word: u64,
    left: u32,
}

impl Keystream {
    fn new(key: WhitenKey) -> Keystream {
        // "Particle" in ASCII keeps the stream distinct from other uses of splitmix64.
        const DOMAIN: u64 = 0x5061_7274_6963_6C65;
        let seed = (u64::from(key.session_id) << 32)
            | (u64::from(key.segment_index) << 16)
            | u64::from(key.packet_seed);
        let mut ks = Keystream {
            state: seed ^ DOMAIN,
            word: 0,
            left: 0,
        };
        // One scrambling round so neighbouring keys do not yield shifted copies of a stream.
        ks.state = ks.next_word();
        ks
    }

    fn next_word(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_byte(&mut self) -> u8 {
        if self.left == 0 {
            self.word = self.next_word();
            self.left = 8;
        }
        let b = self.word as u8;
        self.word >>= 8;
        self.left -= 1;
        b
    }
}

/// Layout and codec for the dot field of one frame at a given dot count and ECC level.
#[derive(Debug, Clone)]
pub struct BodyCodec {
    dots: usize,
    ecc: EccLevel,
    coded_bytes: usize,
    block_size: usize,
    /// One code per codeword, in codeword order.
    codes: Vec<ReedSolomon>,
}

impl BodyCodec {
    /// Layout for `dots` payload dots at `ecc`.
    pub fn new(dots: usize, ecc: EccLevel) -> BodyCodec {
        let coded_bytes = 3 * dots / 8;
        let count = coded_bytes.div_ceil(MAX_CODEWORD);
        let mut codes = Vec::with_capacity(count);
        let mut block_size = 0;
        if let (Some(base), Some(longer)) = (
            coded_bytes.checked_div(count),
            coded_bytes.checked_rem(count),
        ) {
            for c in 0..count {
                let n = base + usize::from(c < longer);
                let parity = (n * ecc.parity_per_255()).div_ceil(MAX_CODEWORD);
                let code = ReedSolomon::new(&GF256, n, parity)
                    .expect("1 <= parity <= n <= 255 holds for every codeword");
                block_size += code.k();
                codes.push(code);
            }
        }
        BodyCodec {
            dots,
            ecc,
            coded_bytes,
            block_size,
            codes,
        }
    }

    /// Payload dots this codec covers.
    pub fn dots(&self) -> usize {
        self.dots
    }

    /// ECC level.
    pub fn ecc(&self) -> EccLevel {
        self.ecc
    }

    /// Bytes on the wire after RS: `B = ⌊3·dots / 8⌋`.
    pub fn coded_bytes(&self) -> usize {
        self.coded_bytes
    }

    /// Number of RS codewords per frame, `C = ⌈B / 255⌉`.
    pub fn codeword_count(&self) -> usize {
        self.codes.len()
    }

    /// Length `n` of codeword `index` (0 if out of range).
    pub fn codeword_len(&self, index: usize) -> usize {
        self.codes.get(index).map_or(0, ReedSolomon::n)
    }

    /// Data bytes per frame (= the `RaptorQ` symbol size `T`).
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    /// Number of stream symbols that carry coded bytes, `⌈8B / 3⌉`.
    fn stream_symbols(&self) -> usize {
        (8 * self.coded_bytes).div_ceil(3)
    }

    /// `(codeword, offset)` of each stream position, in transmission order.
    fn interleave_order(&self) -> Vec<(usize, usize)> {
        let longest = self.codes.first().map_or(0, ReedSolomon::n);
        let mut order = Vec::with_capacity(self.coded_bytes);
        for j in 0..longest {
            for (c, code) in self.codes.iter().enumerate() {
                if j < code.n() {
                    order.push((c, j));
                }
            }
        }
        order
    }

    /// Encode `data` into exactly `dots` symbol values (`0..8`).
    ///
    /// # Panics
    ///
    /// Panics if `data.len() != block_size()`.
    pub fn encode(&self, data: &[u8], key: WhitenKey) -> Vec<u8> {
        assert_eq!(
            data.len(),
            self.block_size,
            "body data must be block_size() bytes"
        );
        let mut ks = Keystream::new(key);
        let whitened: Vec<u8> = data.iter().map(|&b| b ^ ks.next_byte()).collect();

        let mut words = Vec::with_capacity(self.codes.len());
        let mut at = 0;
        for code in &self.codes {
            words.push(code.encode(&whitened[at..at + code.k()]));
            at += code.k();
        }
        let stream: Vec<u8> = self
            .interleave_order()
            .into_iter()
            .map(|(c, j)| words[c][j])
            .collect();

        let used = self.stream_symbols();
        let mut symbols = unpack(&stream, used);
        // Unused low bits of the last stream symbol, then leftover dots: keystream filler.
        let spare_bits = 3 * used - 8 * self.coded_bytes;
        if let (true, Some(last)) = (spare_bits > 0, symbols.last_mut()) {
            *last |= ks.next_byte() & ((1u8 << spare_bits) - 1);
        }
        while symbols.len() < self.dots {
            symbols.push(ks.next_byte() & 7);
        }
        symbols
    }

    /// Decode `symbols` (`dots` of them, in dot order) back into `block_size()` data bytes.
    ///
    /// A coded byte is an erasure if any dot contributing to it has confidence below
    /// [`ERASURE_BELOW`]; each codeword keeps at most `parity` erasures (the least confident)
    /// and is retried without erasures if that fails. Never panics.
    pub fn decode(&self, symbols: &[Sym3], key: WhitenKey) -> Result<Vec<u8>, DecodeError> {
        if symbols.len() != self.dots {
            return Err(DecodeError::WrongSymbolCount {
                expected: self.dots,
                got: symbols.len(),
            });
        }
        let used = self.stream_symbols();
        let values: Vec<u8> = symbols[..used].iter().map(|s| s.value & 7).collect();
        let bytes = crate::symbol::pack(&values);

        // Stream byte `p` covers bits 8p..8p+8, i.e. symbols 8p/3 ..= (8p+7)/3.
        let confidence = |p: usize| -> u8 {
            symbols[8 * p / 3..=(8 * p + 7) / 3]
                .iter()
                .map(|s| s.confidence)
                .min()
                .unwrap_or(0)
        };

        let mut words: Vec<Vec<u8>> = self.codes.iter().map(|c| vec![0u8; c.n()]).collect();
        let mut conf: Vec<Vec<u8>> = self.codes.iter().map(|c| vec![255u8; c.n()]).collect();
        for (p, (c, j)) in self.interleave_order().into_iter().enumerate() {
            words[c][j] = bytes[p];
            conf[c][j] = confidence(p);
        }

        let mut data = Vec::with_capacity(self.block_size);
        for ((code, word), conf) in self.codes.iter().zip(&mut words).zip(&conf) {
            let candidates: Vec<(usize, u8)> = conf
                .iter()
                .enumerate()
                .filter(|&(_, &q)| q < ERASURE_BELOW)
                .map(|(j, &q)| (j, q))
                .collect();
            let erasures = lowest_confidence(candidates, code.parity());
            let received = word.clone();
            if code.decode(word, &erasures).is_err() {
                if erasures.is_empty() {
                    return Err(DecodeError::Uncorrectable);
                }
                word.copy_from_slice(&received);
                code.decode(word, &[])?;
            }
            data.extend_from_slice(&word[..code.k()]);
        }

        let mut ks = Keystream::new(key);
        for b in &mut data {
            *b ^= ks.next_byte();
        }
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            let mut ks = Keystream {
                state: self.0,
                word: 0,
                left: 0,
            };
            let w = ks.next_word();
            self.0 = ks.state;
            w
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    fn key(n: u16) -> WhitenKey {
        WhitenKey {
            session_id: 0xBEEF ^ n,
            segment_index: n,
            packet_seed: n.wrapping_mul(7),
        }
    }

    fn payload(rng: &mut Rng, len: usize) -> Vec<u8> {
        (0..len).map(|_| rng.next() as u8).collect()
    }

    fn certain(symbols: &[u8]) -> Vec<Sym3> {
        symbols.iter().map(|&v| Sym3::new(v)).collect()
    }

    /// Replace `value` by a different symbol, with the given confidence.
    fn damage(rng: &mut Rng, s: &mut Sym3, confidence: u8) {
        s.value = (s.value + 1 + rng.below(7) as u8) & 7;
        s.confidence = confidence;
    }

    #[test]
    fn layout_numbers() {
        // dots, ecc, coded bytes B, codewords C, block size T
        let cases = [
            (2000, EccLevel::Normal, 750, 3, 561),
            (2000, EccLevel::High, 750, 3, 465),
            (4466, EccLevel::Normal, 1674, 7, 1253),
            (8000, EccLevel::Normal, 3000, 12, 2244),
            (4466, EccLevel::High, 1674, 7, 1043),
            (8000, EccLevel::High, 3000, 12, 1860),
        ];
        for (dots, ecc, b, c, t) in cases {
            let codec = BodyCodec::new(dots, ecc);
            assert_eq!(codec.coded_bytes(), b);
            assert_eq!(codec.codeword_count(), c);
            assert_eq!(codec.block_size(), t, "dots {dots} {ecc:?}");
            let total: usize = (0..c).map(|i| codec.codeword_len(i)).sum();
            assert_eq!(total, b);
            assert!(codec.block_size() < b);
        }
    }

    #[test]
    fn full_size_codeword_is_rs_255_191_and_159() {
        let normal = BodyCodec::new(680, EccLevel::Normal); // B = 255
        assert_eq!((normal.codeword_count(), normal.block_size()), (1, 191));
        let high = BodyCodec::new(680, EccLevel::High);
        assert_eq!(high.block_size(), 159);
    }

    #[test]
    fn degenerate_dot_counts() {
        for dots in 0..40 {
            for ecc in [EccLevel::Normal, EccLevel::High] {
                let codec = BodyCodec::new(dots, ecc);
                let mut rng = Rng(dots as u64);
                let data = payload(&mut rng, codec.block_size());
                let syms = codec.encode(&data, key(1));
                assert_eq!(syms.len(), dots);
                assert_eq!(codec.decode(&certain(&syms), key(1)), Ok(data));
            }
        }
    }

    #[test]
    fn roundtrip_typical_sizes() {
        for dots in [2000, 4466, 8000] {
            for ecc in [EccLevel::Normal, EccLevel::High] {
                let codec = BodyCodec::new(dots, ecc);
                let mut rng = Rng(dots as u64);
                let data = payload(&mut rng, codec.block_size());
                let syms = codec.encode(&data, key(3));
                assert_eq!(syms.len(), dots);
                assert!(syms.iter().all(|&s| s < 8));
                assert_eq!(
                    codec.decode(&certain(&syms), key(3)),
                    Ok(data),
                    "{dots} {ecc:?}"
                );
            }
        }
    }

    #[test]
    fn whitening_hides_structure_and_depends_on_key() {
        let codec = BodyCodec::new(4466, EccLevel::Normal);
        let zeros = vec![0u8; codec.block_size()];
        let a = codec.encode(&zeros, key(1));
        let b = codec.encode(&zeros, key(2));
        assert_ne!(a, b);
        assert_eq!(a, codec.encode(&zeros, key(1)), "deterministic");
        // Roughly uniform symbol histogram even for an all-zero payload.
        let mut hist = [0usize; 8];
        for &s in &a {
            hist[s as usize] += 1;
        }
        let expect = a.len() / 8;
        assert!(
            hist.iter()
                .all(|&h| h > expect * 3 / 4 && h < expect * 5 / 4),
            "{hist:?}"
        );
        // Decoding with the wrong key gives garbage, not the payload.
        assert_ne!(codec.decode(&certain(&a), key(2)).unwrap(), zeros);
    }

    #[test]
    fn five_percent_flagged_symbol_errors_are_corrected_at_normal() {
        for dots in [2000, 4466, 8000] {
            let codec = BodyCodec::new(dots, EccLevel::Normal);
            let mut rng = Rng(77 + dots as u64);
            let data = payload(&mut rng, codec.block_size());
            let mut syms = certain(&codec.encode(&data, key(5)));
            for s in &mut syms {
                if rng.below(100) < 5 {
                    let conf = rng.below(usize::from(ERASURE_BELOW)) as u8;
                    damage(&mut rng, s, conf);
                }
            }
            assert_eq!(codec.decode(&syms, key(5)), Ok(data.clone()), "dots {dots}");
        }
    }

    #[test]
    fn two_percent_unflagged_symbol_errors_are_corrected_at_normal() {
        for dots in [2000, 4466, 8000] {
            let codec = BodyCodec::new(dots, EccLevel::Normal);
            let mut rng = Rng(5 + dots as u64);
            let data = payload(&mut rng, codec.block_size());
            let mut syms = certain(&codec.encode(&data, key(6)));
            for s in &mut syms {
                if rng.below(100) < 2 {
                    damage(&mut rng, s, 255);
                }
            }
            assert_eq!(codec.decode(&syms, key(6)), Ok(data.clone()), "dots {dots}");
        }
    }

    #[test]
    fn heavy_damage_is_rejected_not_misdecoded() {
        let codec = BodyCodec::new(2000, EccLevel::Normal);
        let mut rng = Rng(11);
        let data = payload(&mut rng, codec.block_size());
        let mut syms = certain(&codec.encode(&data, key(7)));
        for s in &mut syms {
            if rng.below(100) < 40 {
                damage(&mut rng, s, 255);
            }
        }
        match codec.decode(&syms, key(7)) {
            Ok(out) => assert_eq!(out, data, "a returned body must be the right one"),
            Err(e) => assert_eq!(e, DecodeError::Uncorrectable),
        }
    }

    #[test]
    fn wrong_symbol_count_is_rejected() {
        let codec = BodyCodec::new(2000, EccLevel::Normal);
        assert_eq!(
            codec.decode(&[Sym3::new(0); 10], key(0)),
            Err(DecodeError::WrongSymbolCount {
                expected: 2000,
                got: 10
            })
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(24))]

        #[test]
        fn roundtrip_any_size(
            dots in 0usize..3000,
            high in any::<bool>(),
            seed in any::<u64>(),
            k in (any::<u16>(), any::<u16>(), any::<u16>()),
        ) {
            let ecc = if high { EccLevel::High } else { EccLevel::Normal };
            let codec = BodyCodec::new(dots, ecc);
            let mut rng = Rng(seed);
            let data = payload(&mut rng, codec.block_size());
            let key = WhitenKey { session_id: k.0, segment_index: k.1, packet_seed: k.2 };
            let syms = codec.encode(&data, key);
            prop_assert_eq!(syms.len(), dots);
            prop_assert_eq!(codec.decode(&certain(&syms), key), Ok(data));
        }

        #[test]
        fn random_symbols_never_panic(
            dots in 0usize..1500,
            raw in proptest::collection::vec((any::<u8>(), any::<u8>()), 1500),
        ) {
            let codec = BodyCodec::new(dots, EccLevel::Normal);
            let syms: Vec<Sym3> = raw[..dots]
                .iter()
                .map(|&(value, confidence)| Sym3 { value, confidence })
                .collect();
            let _ = codec.decode(&syms, key(0));
        }
    }
}
