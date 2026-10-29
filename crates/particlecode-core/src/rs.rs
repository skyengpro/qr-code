//! Reed-Solomon codes over [`crate::gf`] with errors-and-erasures decoding, plus the header
//! code RS(24,16) over GF(2⁶).
//!
//! The code is systematic: a codeword is `data ‖ parity`. Its generator polynomial has roots
//! `α^0 .. α^(parity−1)`. Shortened codes (`n` below the field's maximum `2^m − 1`) are
//! supported for any `n` and parity count. Decoding corrects `e` errors and `s` erasures
//! whenever `2e + s ≤ parity`, and verifies its result by recomputing the syndromes, so a
//! detectable failure is reported as [`DecodeError::Uncorrectable`] instead of returning a
//! wrong codeword. Decoding never panics, whatever the input.

use alloc::vec;
use alloc::vec::Vec;

use crate::error::{DecodeError, EncodeError};
use crate::gf::{GF64, Gf};
use crate::header::HEADER_SYMBOLS;
use crate::symbol::Sym3;

/// Symbols whose classifier confidence is below this value are treated as erasures.
pub const ERASURE_BELOW: u8 = 96;

/// Header code length in GF(2⁶) symbols.
const HEADER_N: usize = 24;
/// Header data symbols (96 bits / 6).
const HEADER_K: usize = 16;
/// Header parity symbols.
const HEADER_PARITY: usize = HEADER_N - HEADER_K;

/// A systematic Reed-Solomon code `RS(n, n − parity)` over a [`Gf`].
///
/// Codeword byte `i` is the coefficient of `x^(n−1−i)`; the first `k = n − parity` bytes are
/// data and the last `parity` bytes are parity.
#[derive(Debug, Clone)]
pub struct ReedSolomon {
    gf: &'static Gf,
    n: usize,
    parity: usize,
    /// Generator polynomial, coefficients from the highest degree down (monic, so `gen[0] = 1`
    /// and the length is `parity + 1`).
    gen_poly: Vec<u8>,
}

impl ReedSolomon {
    /// Build a code of codeword length `n` with `parity` parity symbols.
    ///
    /// Errors with [`EncodeError::InvalidConfig`] unless `1 ≤ parity ≤ n ≤ 2^m − 1`.
    pub fn new(gf: &'static Gf, n: usize, parity: usize) -> Result<ReedSolomon, EncodeError> {
        if n == 0 || n > gf.order() {
            return Err(EncodeError::InvalidConfig("RS length must be in 1..=2^m-1"));
        }
        if parity == 0 || parity > n {
            return Err(EncodeError::InvalidConfig("RS parity must be in 1..=n"));
        }
        // g(x) = Π (x − α^j), j = 0..parity, built one root at a time.
        let mut gen_poly = vec![1u8];
        for j in 0..parity {
            let root = gf.exp(j);
            let mut next = vec![0u8; gen_poly.len() + 1];
            for (i, &c) in gen_poly.iter().enumerate() {
                next[i] ^= c;
                next[i + 1] ^= gf.mul(c, root);
            }
            gen_poly = next;
        }
        Ok(ReedSolomon {
            gf,
            n,
            parity,
            gen_poly,
        })
    }

    /// Codeword length `n`.
    pub fn n(&self) -> usize {
        self.n
    }

    /// Data symbols per codeword, `n − parity`.
    pub fn k(&self) -> usize {
        self.n - self.parity
    }

    /// Parity symbols per codeword.
    pub fn parity(&self) -> usize {
        self.parity
    }

    /// The field this code works over.
    pub fn gf(&self) -> &'static Gf {
        self.gf
    }

    /// Compute the `parity` check symbols for `data` (`data.len() == k`).
    ///
    /// Bytes outside the field are reduced to their low `m` bits.
    ///
    /// # Panics
    ///
    /// Panics if `data.len() != k`; that is a caller bug, not an input condition.
    pub fn parity_of(&self, data: &[u8]) -> Vec<u8> {
        assert_eq!(data.len(), self.k(), "RS data length must equal k");
        let mask = self.gf.order() as u8;
        let mut reg = vec![0u8; self.parity];
        for &d in data {
            let feedback = (d & mask) ^ reg[0];
            reg.rotate_left(1);
            reg[self.parity - 1] = 0;
            if feedback != 0 {
                for (r, &g) in reg.iter_mut().zip(&self.gen_poly[1..]) {
                    *r ^= self.gf.mul(feedback, g);
                }
            }
        }
        reg
    }

    /// Systematic encode: returns the full codeword `data ‖ parity` (`n` symbols).
    ///
    /// # Panics
    ///
    /// Panics if `data.len() != k`.
    pub fn encode(&self, data: &[u8]) -> Vec<u8> {
        let mask = self.gf.order() as u8;
        let mut out: Vec<u8> = data.iter().map(|&d| d & mask).collect();
        let parity = self.parity_of(&out);
        out.extend_from_slice(&parity);
        out
    }

    /// Correct `codeword` (`n` symbols) in place. `erasures` lists known-bad positions
    /// (indices into the codeword; duplicates are ignored). Returns the number of symbols
    /// changed.
    ///
    /// Succeeds whenever `2·errors + erasures ≤ parity`. Fails with
    /// [`DecodeError::Uncorrectable`] if the damage is beyond that and detected, if an erasure
    /// index is out of range, if there are more erasures than parity symbols, or if a symbol
    /// is not a field element. On failure `codeword` is left unchanged.
    pub fn decode(&self, codeword: &mut [u8], erasures: &[usize]) -> Result<usize, DecodeError> {
        if codeword.len() != self.n {
            return Err(DecodeError::WrongSymbolCount {
                expected: self.n,
                got: codeword.len(),
            });
        }
        let gf = self.gf;
        if codeword.iter().any(|&c| !gf.contains(c)) {
            return Err(DecodeError::Uncorrectable);
        }
        let mut erased: Vec<usize> = erasures.to_vec();
        erased.sort_unstable();
        erased.dedup();
        if erased.last().is_some_and(|&p| p >= self.n) || erased.len() > self.parity {
            return Err(DecodeError::Uncorrectable);
        }

        let syn = self.syndromes(codeword);
        if syn.iter().all(|&s| s == 0) {
            return Ok(0);
        }

        let lambda = self.errata_locator(&syn, &erased);
        let degree = lambda.iter().rposition(|&c| c != 0).unwrap_or(0);
        if degree == 0 || degree > self.parity {
            return Err(DecodeError::Uncorrectable);
        }

        // Chien search over the n real positions; every root must be one of them.
        let mut positions = Vec::with_capacity(degree);
        for pos in 0..self.n {
            let x_inv = gf.exp(gf.order() - (self.n - 1 - pos));
            if poly_eval(gf, &lambda, x_inv) == 0 {
                positions.push(pos);
            }
        }
        if positions.len() != degree {
            return Err(DecodeError::Uncorrectable);
        }

        // Forney: e = X · Ω(X⁻¹) / Λ'(X⁻¹) with Ω = S·Λ mod x^parity (first root α⁰).
        let mut omega = vec![0u8; self.parity];
        for (i, o) in omega.iter_mut().enumerate() {
            for (j, &l) in lambda.iter().enumerate().take(i + 1) {
                *o ^= gf.mul(l, syn[i - j]);
            }
        }
        let mut fixed = codeword.to_vec();
        let mut changed = 0;
        for &pos in &positions {
            let x = gf.exp(self.n - 1 - pos);
            let x_inv = gf.inv(x);
            let denom = formal_derivative_eval(gf, &lambda, x_inv);
            if denom == 0 {
                return Err(DecodeError::Uncorrectable);
            }
            let magnitude = gf.mul(x, gf.div(poly_eval(gf, &omega, x_inv), denom));
            if magnitude != 0 {
                fixed[pos] ^= magnitude;
                changed += 1;
            }
        }

        if self.syndromes(&fixed).iter().any(|&s| s != 0) {
            return Err(DecodeError::Uncorrectable);
        }
        codeword.copy_from_slice(&fixed);
        Ok(changed)
    }

    /// `S_j = c(α^j)` for `j = 0..parity`.
    fn syndromes(&self, codeword: &[u8]) -> Vec<u8> {
        (0..self.parity)
            .map(|j| {
                let a = self.gf.exp(j);
                codeword.iter().fold(0u8, |acc, &c| self.gf.mul(acc, a) ^ c)
            })
            .collect()
    }

    /// Berlekamp-Massey seeded with the erasure locator (Blahut's errors-and-erasures form).
    /// Returns Λ(x) with the lowest degree first, padded to `parity + 1` coefficients.
    fn errata_locator(&self, syn: &[u8], erased: &[usize]) -> Vec<u8> {
        let gf = self.gf;
        let w = self.parity + 1;
        let rho = erased.len();

        // Γ(x) = Π (1 + X_i x) over the erased positions.
        let mut lambda = vec![0u8; w];
        lambda[0] = 1;
        for &pos in erased {
            let x = gf.exp(self.n - 1 - pos);
            for i in (1..w).rev() {
                lambda[i] ^= gf.mul(x, lambda[i - 1]);
            }
        }

        let mut b = lambda.clone();
        let mut l = rho;
        for r in rho..self.parity {
            let mut delta = 0u8;
            for i in 0..=l.min(r).min(w - 1) {
                delta ^= gf.mul(lambda[i], syn[r - i]);
            }
            // b <- x·b (the top coefficient falls off; it can only touch degrees > parity).
            b.rotate_right(1);
            b[0] = 0;
            if delta == 0 {
                continue;
            }
            if 2 * l <= r + rho {
                let old = lambda.clone();
                for (lc, &bc) in lambda.iter_mut().zip(&b) {
                    *lc ^= gf.mul(delta, bc);
                }
                l = r + 1 + rho - l;
                let scale = gf.inv(delta);
                for (bc, &oc) in b.iter_mut().zip(&old) {
                    *bc = gf.mul(scale, oc);
                }
            } else {
                for (lc, &bc) in lambda.iter_mut().zip(&b) {
                    *lc ^= gf.mul(delta, bc);
                }
            }
        }
        lambda
    }
}

/// Evaluate a polynomial stored lowest degree first.
fn poly_eval(gf: &Gf, poly: &[u8], x: u8) -> u8 {
    poly.iter().rev().fold(0u8, |acc, &c| gf.mul(acc, x) ^ c)
}

/// Evaluate the formal derivative (characteristic 2: odd terms only) of a polynomial stored
/// lowest degree first.
fn formal_derivative_eval(gf: &Gf, poly: &[u8], x: u8) -> u8 {
    let mut acc = 0u8;
    let mut i = 1;
    while i < poly.len() {
        acc ^= gf.mul(poly[i], gf.pow(x, i - 1));
        i += 2;
    }
    acc
}

/// Choose at most `max` erasures out of `(index, confidence)` candidates, keeping the
/// least confident ones (ties broken by lower index). Returns indices in ascending order.
pub(crate) fn lowest_confidence(mut candidates: Vec<(usize, u8)>, max: usize) -> Vec<usize> {
    if candidates.len() > max {
        candidates.sort_unstable_by_key(|&(idx, conf)| (conf, idx));
        candidates.truncate(max);
    }
    let mut out: Vec<usize> = candidates.into_iter().map(|(idx, _)| idx).collect();
    out.sort_unstable();
    out
}

fn header_code() -> ReedSolomon {
    ReedSolomon::new(&GF64, HEADER_N, HEADER_PARITY).expect("header code parameters are valid")
}

/// Encode the 12 header bytes into 48 dot symbols (RS(24,16) over GF(2⁶), symbol `i` →
/// dots `2i` (high 3 bits) and `2i+1` (low 3 bits)).
pub fn header_encode(bytes: &[u8; 12]) -> [u8; HEADER_SYMBOLS] {
    let data: Vec<u8> = (0..HEADER_K).map(|i| six_bits(bytes, i)).collect();
    let code = header_code().encode(&data);
    let mut dots = [0u8; HEADER_SYMBOLS];
    for (i, &sym) in code.iter().enumerate() {
        dots[2 * i] = sym >> 3;
        dots[2 * i + 1] = sym & 7;
    }
    dots
}

/// Inverse of [`header_encode`]. Dots with low confidence become erasures of their symbol.
///
/// A GF(2⁶) symbol is erased if either of its two dots has confidence below
/// [`ERASURE_BELOW`]. With more erasures than parity symbols only the least confident ones
/// are kept. If decoding with erasures fails it is retried once without.
pub fn header_decode(symbols: &[Sym3]) -> Result<[u8; 12], DecodeError> {
    if symbols.len() != HEADER_SYMBOLS {
        return Err(DecodeError::WrongSymbolCount {
            expected: HEADER_SYMBOLS,
            got: symbols.len(),
        });
    }
    let mut code = [0u8; HEADER_N];
    let mut candidates = Vec::new();
    for (i, pair) in symbols.chunks_exact(2).enumerate() {
        code[i] = ((pair[0].value & 7) << 3) | (pair[1].value & 7);
        let conf = pair[0].confidence.min(pair[1].confidence);
        if conf < ERASURE_BELOW {
            candidates.push((i, conf));
        }
    }
    let erasures = lowest_confidence(candidates, HEADER_PARITY);

    let rs = header_code();
    let mut attempt = code;
    if rs.decode(&mut attempt, &erasures).is_err() {
        attempt = code;
        if erasures.is_empty() {
            return Err(DecodeError::Uncorrectable);
        }
        rs.decode(&mut attempt, &[])?;
    }

    let mut out = [0u8; 12];
    for (i, &sym) in attempt[..HEADER_K].iter().enumerate() {
        for b in 0..6 {
            let bit = (sym >> (5 - b)) & 1;
            let pos = i * 6 + b;
            out[pos / 8] |= bit << (7 - pos % 8);
        }
    }
    Ok(out)
}

/// The `i`-th 6-bit group (MSB-first) of `bytes`.
fn six_bits(bytes: &[u8; 12], i: usize) -> u8 {
    let mut v = 0u8;
    for b in 0..6 {
        let pos = i * 6 + b;
        v = (v << 1) | ((bytes[pos / 8] >> (7 - pos % 8)) & 1);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gf::GF256;
    use proptest::prelude::*;

    /// Tiny deterministic generator so proptest only has to supply a seed.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }

        /// `count` distinct indices below `n`.
        fn distinct(&mut self, n: usize, count: usize) -> Vec<usize> {
            let mut pool: Vec<usize> = (0..n).collect();
            let mut out = Vec::new();
            for _ in 0..count.min(n) {
                let i = self.below(pool.len());
                out.push(pool.swap_remove(i));
            }
            out
        }
    }

    fn random_data(rng: &mut Rng, gf: &Gf, k: usize) -> Vec<u8> {
        (0..k)
            .map(|_| (rng.next() as u8) & gf.order() as u8)
            .collect()
    }

    /// Flip `pos` to a different value.
    fn corrupt(rng: &mut Rng, gf: &Gf, word: &mut [u8], pos: usize) {
        let delta = 1 + (rng.below(gf.order()) as u8);
        word[pos] ^= delta;
    }

    #[test]
    fn rejects_bad_parameters() {
        assert!(ReedSolomon::new(&GF64, 64, 8).is_err());
        assert!(ReedSolomon::new(&GF64, 24, 0).is_err());
        assert!(ReedSolomon::new(&GF64, 24, 25).is_err());
        assert!(ReedSolomon::new(&GF256, 0, 1).is_err());
        assert!(ReedSolomon::new(&GF256, 255, 64).is_ok());
    }

    #[test]
    fn codewords_have_zero_syndromes() {
        let rs = ReedSolomon::new(&GF256, 255, 64).unwrap();
        let mut rng = Rng(1);
        let cw = rs.encode(&random_data(&mut rng, &GF256, rs.k()));
        assert_eq!(cw.len(), 255);
        assert!(rs.syndromes(&cw).iter().all(|&s| s == 0));
    }

    #[test]
    fn clean_codeword_decodes_with_zero_corrections() {
        let rs = ReedSolomon::new(&GF256, 100, 20).unwrap();
        let mut cw = rs.encode(&[7; 80]);
        let orig = cw.clone();
        assert_eq!(rs.decode(&mut cw, &[]), Ok(0));
        assert_eq!(rs.decode(&mut cw, &[3, 4]), Ok(0));
        assert_eq!(cw, orig);
    }

    #[test]
    fn bad_erasure_arguments_are_rejected() {
        let rs = ReedSolomon::new(&GF256, 30, 6).unwrap();
        let mut cw = rs.encode(&[1; 24]);
        cw[0] ^= 1;
        let before = cw.clone();
        assert_eq!(rs.decode(&mut cw, &[30]), Err(DecodeError::Uncorrectable));
        assert_eq!(
            rs.decode(&mut cw, &[usize::MAX]),
            Err(DecodeError::Uncorrectable)
        );
        assert_eq!(
            rs.decode(&mut cw, &[0, 1, 2, 3, 4, 5, 6]),
            Err(DecodeError::Uncorrectable)
        );
        assert_eq!(cw, before, "failed decode leaves the word untouched");
        assert!(matches!(
            rs.decode(&mut cw[..10], &[]),
            Err(DecodeError::WrongSymbolCount {
                expected: 30,
                got: 10
            })
        ));
        // Duplicate erasure indices count once.
        assert_eq!(rs.decode(&mut cw, &[0, 0, 0]), Ok(1));
    }

    #[test]
    fn out_of_field_symbols_are_rejected() {
        let rs = ReedSolomon::new(&GF64, 24, 8).unwrap();
        let mut cw = rs.encode(&[5; 16]);
        cw[3] = 200;
        assert_eq!(rs.decode(&mut cw, &[]), Err(DecodeError::Uncorrectable));
    }

    #[test]
    fn full_erasure_budget_and_full_error_budget() {
        let rs = ReedSolomon::new(&GF256, 255, 64).unwrap();
        let mut rng = Rng(9);
        let cw = rs.encode(&random_data(&mut rng, &GF256, rs.k()));

        let mut word = cw.clone();
        let pos = rng.distinct(255, 64);
        for &p in &pos {
            word[p] = 0;
        }
        assert!(rs.decode(&mut word, &pos).is_ok());
        assert_eq!(word, cw);

        let mut word = cw.clone();
        for p in rng.distinct(255, 32) {
            corrupt(&mut rng, &GF256, &mut word, p);
        }
        assert_eq!(rs.decode(&mut word, &[]), Ok(32));
        assert_eq!(word, cw);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        #[test]
        fn roundtrip_and_errors_within_bound(
            seed in any::<u64>(),
            wide in any::<bool>(),
            n_raw in 1usize..=255,
            parity_raw in 1usize..=96,
        ) {
            let gf: &'static Gf = if wide { &GF256 } else { &GF64 };
            let n = n_raw.min(gf.order());
            let parity = parity_raw.min(n);
            let rs = ReedSolomon::new(gf, n, parity).unwrap();
            let mut rng = Rng(seed);
            let cw = rs.encode(&random_data(&mut rng, gf, rs.k()));

            // Errors only, up to t = parity / 2.
            let t = parity / 2;
            let e = rng.below(t + 1);
            let mut word = cw.clone();
            for p in rng.distinct(n, e) {
                corrupt(&mut rng, gf, &mut word, p);
            }
            prop_assert_eq!(rs.decode(&mut word, &[]), Ok(e));
            prop_assert_eq!(&word, &cw);
        }

        #[test]
        fn errors_and_erasures_within_bound(
            seed in any::<u64>(),
            wide in any::<bool>(),
            n_raw in 1usize..=255,
            parity_raw in 1usize..=96,
        ) {
            let gf: &'static Gf = if wide { &GF256 } else { &GF64 };
            let n = n_raw.min(gf.order());
            let parity = parity_raw.min(n);
            let rs = ReedSolomon::new(gf, n, parity).unwrap();
            let mut rng = Rng(seed);
            let cw = rs.encode(&random_data(&mut rng, gf, rs.k()));

            // Pick s erasures, then as many errors as the remaining budget allows.
            let s = rng.below(parity + 1);
            let e = rng.below((parity - s) / 2 + 1);
            let all = rng.distinct(n, (s + e).min(n));
            let (era, err) = all.split_at(s.min(all.len()));
            let mut word = cw.clone();
            for &p in era {
                word[p] = rng.next() as u8 & gf.order() as u8; // arbitrary, maybe unchanged
            }
            for &p in err {
                corrupt(&mut rng, gf, &mut word, p);
            }
            prop_assert!(rs.decode(&mut word, era).is_ok());
            prop_assert_eq!(&word, &cw);
        }

        #[test]
        fn beyond_the_bound_never_panics_or_lies(
            seed in any::<u64>(),
            n in 2usize..=255,
            parity_raw in 2usize..=64,
            extra_errors in 1usize..40,
            erasure_noise in 0usize..80,
        ) {
            let parity = parity_raw.min(n);
            let rs = ReedSolomon::new(&GF256, n, parity).unwrap();
            let mut rng = Rng(seed);
            let cw = rs.encode(&random_data(&mut rng, &GF256, rs.k()));
            let mut word = cw.clone();
            for p in rng.distinct(n, (parity / 2 + extra_errors).min(n)) {
                corrupt(&mut rng, &GF256, &mut word, p);
            }
            // Erasure indices may be duplicated, out of range, or too many.
            let erasures: Vec<usize> =
                (0..erasure_noise).map(|_| rng.below(n + 8)).collect();
            let before = word.clone();
            if rs.decode(&mut word, &erasures).is_ok() {
                // Whatever it returns must be a genuine codeword.
                prop_assert!(rs.syndromes(&word).iter().all(|&s| s == 0));
            } else {
                prop_assert_eq!(word, before);
            }
        }

        #[test]
        fn random_garbage_never_panics(
            garbage in proptest::collection::vec(any::<u8>(), 24),
            erasures in proptest::collection::vec(0usize..40, 0..12),
        ) {
            let rs = ReedSolomon::new(&GF64, 24, 8).unwrap();
            let mut word = garbage;
            let _ = rs.decode(&mut word, &erasures);
        }
    }

    // ---- header code ----

    fn header_dots(bytes: &[u8; 12]) -> Vec<Sym3> {
        header_encode(bytes).iter().map(|&v| Sym3::new(v)).collect()
    }

    fn sample_bytes(seed: u64) -> [u8; 12] {
        let mut rng = Rng(seed);
        let mut b = [0u8; 12];
        for x in &mut b {
            *x = rng.next() as u8;
        }
        b
    }

    #[test]
    fn header_is_systematic_in_the_first_32_dots() {
        let bytes = sample_bytes(3);
        let dots = header_encode(&bytes);
        assert!(dots.iter().all(|&d| d < 8));
        // First data symbol = first six bits of the header.
        let sym0 = (dots[0] << 3) | dots[1];
        assert_eq!(sym0, bytes[0] >> 2);
    }

    #[test]
    fn header_roundtrip_clean() {
        for seed in 0..50 {
            let bytes = sample_bytes(seed);
            assert_eq!(header_decode(&header_dots(&bytes)), Ok(bytes));
        }
    }

    #[test]
    fn header_wrong_length_is_rejected() {
        assert!(matches!(
            header_decode(&[Sym3::new(0); 47]),
            Err(DecodeError::WrongSymbolCount {
                expected: 48,
                got: 47
            })
        ));
    }

    #[test]
    fn header_survives_a_lost_halo() {
        // Dots 32..48 are the outer halo: symbols 16..24, i.e. all eight parity symbols.
        // Also try each other aligned block of 16 dots.
        for halo in 0..3 {
            for seed in 0..20 {
                let bytes = sample_bytes(seed);
                let mut dots = header_dots(&bytes);
                for d in &mut dots[halo * 16..halo * 16 + 16] {
                    *d = Sym3 {
                        value: 7 - d.value,
                        confidence: 0,
                    };
                }
                assert_eq!(header_decode(&dots), Ok(bytes), "halo {halo} seed {seed}");
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn header_corrects_four_symbol_errors(seed in any::<u64>(), flips in 0usize..=4) {
            let mut rng = Rng(seed);
            let bytes = sample_bytes(rng.next());
            let mut dots = header_dots(&bytes);
            for sym in rng.distinct(HEADER_N, flips) {
                // Damage either or both dots of the GF(2^6) symbol, full confidence.
                let which = 1 + rng.below(3);
                if which & 1 != 0 {
                    dots[2 * sym].value ^= 1 + rng.below(7) as u8;
                }
                if which & 2 != 0 {
                    dots[2 * sym + 1].value ^= 1 + rng.below(7) as u8;
                }
            }
            prop_assert_eq!(header_decode(&dots), Ok(bytes));
        }

        #[test]
        fn header_corrects_errors_plus_erasures(seed in any::<u64>(), s in 0usize..=8) {
            let mut rng = Rng(seed);
            let bytes = sample_bytes(rng.next());
            let mut dots = header_dots(&bytes);
            let e = rng.below((8 - s) / 2 + 1);
            let picks = rng.distinct(HEADER_N, s + e);
            for (i, &sym) in picks.iter().enumerate() {
                let d = 2 * sym + rng.below(2);
                dots[d].value ^= 1 + rng.below(7) as u8;
                if i < s {
                    dots[d].confidence = rng.below(usize::from(ERASURE_BELOW)) as u8;
                }
            }
            prop_assert_eq!(header_decode(&dots), Ok(bytes));
        }

        #[test]
        fn header_decode_never_panics(
            raw in proptest::collection::vec((any::<u8>(), any::<u8>()), 48),
        ) {
            let dots: Vec<Sym3> =
                raw.into_iter().map(|(value, confidence)| Sym3 { value, confidence }).collect();
            let _ = header_decode(&dots);
        }
    }
}
