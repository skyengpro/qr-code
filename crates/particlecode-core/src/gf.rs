//! Binary extension fields GF(2^m), `m ≤ 8`, for Reed-Solomon: GF(2⁶) for the header and
//! GF(2⁸) for the body.
//!
//! Elements are plain `u8` values below `2^m`. Addition is XOR; multiplication goes through
//! exp/log tables that are built at compile time ([`Gf::new`] is a `const fn`), so there is no
//! runtime initialisation and the module is `no_std`-friendly.

/// One finite field GF(2^m) with its primitive element `α = x`.
///
/// Use the ready-made [`GF64`] and [`GF256`] statics. Functions that take field elements
/// expect values below [`Gf::size`]; out-of-range inputs never panic but give meaningless
/// results.
#[derive(Debug, Clone)]
pub struct Gf {
    m: u32,
    /// Multiplicative group order, `2^m − 1`.
    order: usize,
    /// `exp[i] = α^i`, doubled in length so `exp[log a + log b]` needs no reduction.
    exp: [u8; 512],
    /// `log[a]` for `a ≠ 0` (`log[0]` is unused and holds 0).
    log: [u8; 256],
}

impl Gf {
    /// Build GF(2^m) from a primitive polynomial given with its leading bit (`0x43` for
    /// x⁶ + x + 1). Evaluated at compile time for the statics below.
    ///
    /// # Panics
    ///
    /// Panics (at compile time for `const` use) if `m` is not in `1..=8` or the polynomial
    /// is not primitive of degree `m`.
    pub const fn new(m: u32, poly: u16) -> Gf {
        assert!(m >= 1 && m <= 8, "GF(2^m) supports 1 <= m <= 8");
        assert!(poly >> m == 1, "polynomial must have degree m");
        let order = (1usize << m) - 1;
        let mut exp = [0u8; 512];
        let mut log = [0u8; 256];
        let mut x: u16 = 1;
        let mut i = 0;
        while i < order {
            exp[i] = x as u8;
            log[x as usize] = i as u8;
            x <<= 1;
            if x >> m != 0 {
                x ^= poly;
            }
            i += 1;
        }
        assert!(x == 1, "polynomial is not primitive");
        let mut i = 0;
        while i < order {
            exp[i + order] = exp[i];
            i += 1;
        }
        Gf { m, order, exp, log }
    }

    /// Bits per symbol, `m`.
    pub const fn bits(&self) -> u32 {
        self.m
    }

    /// Number of field elements, `2^m`.
    pub const fn size(&self) -> usize {
        self.order + 1
    }

    /// Order of the multiplicative group, `2^m − 1` (also the longest Reed-Solomon codeword).
    pub const fn order(&self) -> usize {
        self.order
    }

    /// Whether `a` is a valid element (`a < 2^m`).
    pub const fn contains(&self, a: u8) -> bool {
        (a as usize) <= self.order
    }

    /// Field addition (= subtraction): XOR.
    pub const fn add(&self, a: u8, b: u8) -> u8 {
        a ^ b
    }

    /// Field multiplication.
    pub const fn mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            0
        } else {
            self.exp[self.log[a as usize] as usize + self.log[b as usize] as usize]
        }
    }

    /// Field division `a / b`. Division by zero yields 0 (callers check for zero first).
    pub const fn div(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            0
        } else {
            self.exp[self.log[a as usize] as usize + self.order - self.log[b as usize] as usize]
        }
    }

    /// Multiplicative inverse. `inv(0)` is 0.
    pub const fn inv(&self, a: u8) -> u8 {
        if a == 0 { 0 } else { self.exp[self.order - self.log[a as usize] as usize] }
    }

    /// `a^e`. `pow(0, 0)` is 1 and `pow(0, e > 0)` is 0.
    pub const fn pow(&self, a: u8, e: usize) -> u8 {
        if e == 0 {
            1
        } else if a == 0 {
            0
        } else {
            let l = self.log[a as usize] as usize;
            self.exp[(l * (e % self.order)) % self.order]
        }
    }

    /// `α^i`, with `i` taken modulo the group order.
    pub const fn exp(&self, i: usize) -> u8 {
        self.exp[i % self.order]
    }

    /// Discrete logarithm base `α`, in `0..order`. `log(0)` is 0 (callers check for zero).
    pub const fn log(&self, a: u8) -> usize {
        self.log[a as usize] as usize
    }
}

/// GF(2⁶), primitive polynomial x⁶ + x + 1 (`0x43`). Used for the header code.
pub static GF64: Gf = Gf::new(6, 0x43);

/// GF(2⁸), primitive polynomial x⁸ + x⁴ + x³ + x² + 1 (`0x11D`). Used for the body codes.
pub static GF256: Gf = Gf::new(8, 0x11D);

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn fields() -> [&'static Gf; 2] {
        [&GF64, &GF256]
    }

    #[test]
    fn sizes() {
        assert_eq!(GF64.size(), 64);
        assert_eq!(GF256.size(), 256);
        assert_eq!(GF64.order(), 63);
        assert_eq!(GF256.order(), 255);
    }

    #[test]
    fn alpha_generates_every_nonzero_element_once() {
        for gf in fields() {
            let mut seen = [false; 256];
            for i in 0..gf.order() {
                let e = gf.exp(i) as usize;
                assert!(e != 0 && gf.contains(e as u8));
                assert!(!seen[e], "duplicate power");
                seen[e] = true;
                assert_eq!(gf.log(e as u8), i);
            }
            assert_eq!(gf.exp(gf.order()), 1);
        }
    }

    #[test]
    fn exhaustive_inverses_and_identities() {
        for gf in fields() {
            for a in 0..gf.size() {
                let a = a as u8;
                assert_eq!(gf.mul(a, 1), a);
                assert_eq!(gf.mul(a, 0), 0);
                assert_eq!(gf.add(a, a), 0);
                if a != 0 {
                    assert_eq!(gf.mul(a, gf.inv(a)), 1);
                    assert_eq!(gf.div(a, a), 1);
                    assert_eq!(gf.pow(a, gf.order()), 1);
                }
            }
        }
    }

    #[test]
    fn known_products() {
        // x * x^7 = x^8 = x^4 + x^3 + x^2 + 1 for 0x11D; x * x^5 = x^6 = x + 1 for 0x43.
        assert_eq!(GF256.mul(2, 0x80), 0x1D);
        assert_eq!(GF64.mul(2, 0x20), 0x03);
    }

    #[test]
    fn degenerate_inputs_do_not_panic() {
        for gf in fields() {
            assert_eq!(gf.div(5, 0), 0);
            assert_eq!(gf.inv(0), 0);
            assert_eq!(gf.pow(0, 0), 1);
            assert_eq!(gf.pow(0, 3), 0);
            assert_eq!(gf.log(0), 0);
        }
    }

    proptest! {
        #[test]
        fn field_axioms(a in 0u8..=255, b in 0u8..=255, c in 0u8..=255) {
            for gf in fields() {
                let mask = gf.order() as u8;
                let (a, b, c) = (a & mask, b & mask, c & mask);
                prop_assert_eq!(gf.mul(a, b), gf.mul(b, a));
                prop_assert_eq!(gf.mul(gf.mul(a, b), c), gf.mul(a, gf.mul(b, c)));
                prop_assert_eq!(
                    gf.mul(a, gf.add(b, c)),
                    gf.add(gf.mul(a, b), gf.mul(a, c))
                );
                if b != 0 {
                    prop_assert_eq!(gf.mul(gf.div(a, b), b), a);
                }
                prop_assert!(gf.contains(gf.mul(a, b)));
            }
        }

        #[test]
        fn pow_matches_repeated_multiplication(a in 0u8..=255, e in 0usize..40) {
            for gf in fields() {
                let a = a & gf.order() as u8;
                let mut acc = 1u8;
                for _ in 0..e {
                    acc = gf.mul(acc, a);
                }
                prop_assert_eq!(gf.pow(a, e), acc);
            }
        }
    }
}
