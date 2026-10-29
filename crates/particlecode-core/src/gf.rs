//! Binary extension fields GF(2^m) for Reed-Solomon: GF(2⁶) for the header and GF(2⁸)
//! for the body.
//!
//! STUB — implemented by the codec work package. Required public surface:
//!
//! * `pub struct Gf { .. }` describing one field (size, exp/log tables), with
//!   `pub const fn`/`pub fn` constructors and `add`, `mul`, `div`, `inv`, `pow`, `exp`, `log`.
//! * `pub static GF64: Gf` (primitive polynomial x⁶ + x + 1, 0x43) and
//!   `pub static GF256: Gf` (x⁸ + x⁴ + x³ + x² + 1, 0x11D), built at compile time.
