//! Particle liveness: six particles × 16 sectors = 24 bits = packet seed low byte ‖
//! 16-bit HMAC-SHA256(session key, second counter).
//!
//! The sender shows a *rolling value* that changes every second and that only a holder of
//! the session key can predict. The receiver can read the 24 bits off the particles
//! ([`decode_sectors`]) but can only check them ([`verify`]) when the application layer
//! has shared the session key with it. The data path never depends on particles.
//!
//! Particle `k` (0..6) shows a nibble of the 24-bit value `seed_low ‖ rolling`, particle 0
//! holding the most significant nibble.

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

/// Number of particles (and so nibbles in the liveness value).
pub const PARTICLES: usize = crate::layout::dims::PARTICLE_COUNT;

/// What the receiver read off the particles of one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Liveness {
    /// The particles could not be read (or the frame was not looked at).
    #[default]
    Unknown,
    /// The particles were read.
    Read {
        /// Low byte of the packet seed, as shown by the particles.
        seed_low: u8,
        /// The 16-bit rolling HMAC value, as shown by the particles.
        rolling: u16,
        /// Whether `seed_low` equals the low byte of the decoded header's packet seed
        /// (a cheap check that needs no key).
        consistent: bool,
    },
}

impl Liveness {
    /// Build a [`Liveness::Read`] from decoded sectors and the decoded header's packet seed.
    pub fn from_sectors(sectors: &[u8; PARTICLES], packet_seed: u16) -> Liveness {
        let (seed_low, rolling) = decode_sectors(sectors);
        Liveness::Read {
            seed_low,
            rolling,
            consistent: seed_low == (packet_seed & 0xFF) as u8,
        }
    }
}

/// The 16-bit rolling value for `unix_second`: the first two bytes (big-endian) of
/// HMAC-SHA256(`key`, `unix_second` as a big-endian `u64`).
pub fn rolling_value(key: &[u8; 32], unix_second: u64) -> u16 {
    let mut mac =
        <Hmac<Sha256> as KeyInit>::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(&unix_second.to_be_bytes());
    let tag = mac.finalize().into_bytes();
    u16::from_be_bytes([tag[0], tag[1]])
}

/// Sector (`0..16`) shown by each particle: the nibbles of `seed_low ‖ rolling`, particle 0
/// holding the top nibble.
pub fn particle_sectors(seed_low: u8, rolling: u16) -> [u8; PARTICLES] {
    let value = (u32::from(seed_low) << 16) | u32::from(rolling);
    let mut out = [0u8; PARTICLES];
    for (k, sector) in out.iter_mut().enumerate() {
        *sector = ((value >> (4 * (PARTICLES - 1 - k))) & 0xF) as u8;
    }
    out
}

/// Inverse of [`particle_sectors`]: `(seed_low, rolling)`. Sectors are masked to 4 bits, so
/// any input decodes.
pub fn decode_sectors(sectors: &[u8; PARTICLES]) -> (u8, u16) {
    let value = sectors
        .iter()
        .fold(0u32, |acc, &s| (acc << 4) | u32::from(s & 0xF));
    ((value >> 16) as u8, value as u16)
}

/// Whether `rolling` is the value for some second within `skew_seconds` of `now_unix`.
///
/// `skew_seconds = 0` accepts the current second only. The window is clamped at the ends of
/// the `u64` range.
pub fn verify(key: &[u8; 32], rolling: u16, now_unix: u64, skew_seconds: u64) -> bool {
    let lo = now_unix.saturating_sub(skew_seconds);
    let hi = now_unix.saturating_add(skew_seconds);
    (lo..=hi).any(|second| rolling_value(key, second) == rolling)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use proptest::prelude::*;

    const KEY: [u8; 32] = [7; 32];

    /// Reference computed independently of `rolling_value`'s code path (one-shot MAC).
    fn reference(key: &[u8; 32], second: u64) -> u16 {
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(key).unwrap();
        mac.update(&second.to_be_bytes());
        let tag = mac.finalize().into_bytes();
        (u16::from(tag[0]) << 8) | u16::from(tag[1])
    }

    #[test]
    fn rolling_is_hmac_prefix() {
        for t in [0, 1, 1_700_000_000, u64::MAX] {
            assert_eq!(rolling_value(&KEY, t), reference(&KEY, t));
        }
    }

    #[test]
    fn rolling_depends_on_key_and_second() {
        let values: Vec<u16> = (0..32).map(|t| rolling_value(&KEY, t)).collect();
        // 32 draws from 65536 values: a repeat would point at a broken input mapping.
        let mut sorted = values.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), values.len());
        assert_ne!(rolling_value(&KEY, 1), rolling_value(&[8; 32], 1));
        assert_eq!(rolling_value(&KEY, 12345), rolling_value(&KEY, 12345));
    }

    #[test]
    fn sectors_layout() {
        assert_eq!(
            particle_sectors(0xAB, 0xCDEF),
            [0xA, 0xB, 0xC, 0xD, 0xE, 0xF]
        );
        assert_eq!(
            decode_sectors(&[0xA, 0xB, 0xC, 0xD, 0xE, 0xF]),
            (0xAB, 0xCDEF)
        );
        assert_eq!(particle_sectors(0, 0), [0; 6]);
        assert_eq!(particle_sectors(0xFF, 0xFFFF), [0xF; 6]);
    }

    #[test]
    fn decode_masks_out_of_range_sectors() {
        assert_eq!(decode_sectors(&[0x1A, 0, 0, 0, 0, 0]), (0xA0, 0));
    }

    #[test]
    fn verify_window() {
        let now = 1_000_000;
        let r = rolling_value(&KEY, now);
        assert!(verify(&KEY, r, now, 0));
        // Pick a probe second whose value differs from `r` (16-bit values can collide).
        let probe = (1..50).find(|d| rolling_value(&KEY, now + d) != r).unwrap();
        let off = rolling_value(&KEY, now + probe);
        assert!(verify(&KEY, off, now, probe));
        let others_in_window = (0..probe).any(|d| rolling_value(&KEY, now + d) == off);
        if !others_in_window && rolling_value(&KEY, now - 1) != off {
            assert!(!verify(&KEY, off, now, probe - 1));
        }
        assert!(verify(&KEY, rolling_value(&KEY, now - 2), now, 2));
    }

    #[test]
    fn verify_extremes_do_not_overflow() {
        let r = rolling_value(&KEY, u64::MAX);
        assert!(verify(&KEY, r, u64::MAX, 1));
        assert!(verify(&KEY, rolling_value(&KEY, 0), 0, 1));
    }

    #[test]
    fn liveness_consistency() {
        let s = particle_sectors(0x23, 0x4567);
        assert_eq!(
            Liveness::from_sectors(&s, 0x0123),
            Liveness::Read {
                seed_low: 0x23,
                rolling: 0x4567,
                consistent: true
            }
        );
        assert_eq!(
            Liveness::from_sectors(&s, 0x0124),
            Liveness::Read {
                seed_low: 0x23,
                rolling: 0x4567,
                consistent: false
            }
        );
    }

    proptest! {
        #[test]
        fn sectors_roundtrip(seed in any::<u8>(), rolling in any::<u16>()) {
            let s = particle_sectors(seed, rolling);
            prop_assert!(s.iter().all(|&x| x < 16));
            prop_assert_eq!(decode_sectors(&s), (seed, rolling));
        }

        #[test]
        fn verify_accepts_own_value(
            key in any::<[u8; 32]>(),
            t in 10u64..u64::MAX - 10,
            skew in 0u64..4,
        ) {
            let r = rolling_value(&key, t);
            prop_assert!(verify(&key, r, t, skew));
            prop_assert!(verify(&key, r, t + skew, skew));
            prop_assert!(verify(&key, r, t - skew, skew));
        }
    }
}
