//! Liveness as read from the particles.
//!
//! This is a local mirror of the type the session work package defines in
//! `particlecode_core::liveness`; swapping it for the core type is a one-line change in
//! `lib.rs` plus the `from_sectors` constructor.

use particlecode_core::layout::dims;

/// What the six orbiting particles say about the sender being live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    /// The particles could not be read (hidden, cut off, ambiguous).
    Unknown,
    /// The 24 bits the particles carry: packet seed low byte ‖ 16-bit rolling value.
    Read {
        /// Low 8 bits of the packet seed.
        seed_low: u8,
        /// The rolling HMAC-derived value; verifying it needs the session key.
        rolling: u16,
        /// Whether `seed_low` equals the low byte of the frame header's packet seed.
        consistent: bool,
    },
}

impl Liveness {
    /// Assemble from the six sectors (particle 0 holds the top nibble).
    ///
    /// `packet_seed` is the header's packet seed when the header is known; without it
    /// `consistent` is `true` (nothing to contradict).
    pub fn from_sectors(
        sectors: Option<[u8; dims::PARTICLE_COUNT]>,
        packet_seed: Option<u16>,
    ) -> Liveness {
        let Some(s) = sectors else {
            return Liveness::Unknown;
        };
        let value = s
            .iter()
            .fold(0u32, |acc, &n| (acc << 4) | u32::from(n & 15));
        let seed_low = (value >> 16) as u8;
        let rolling = (value & 0xFFFF) as u16;
        let consistent = packet_seed.is_none_or(|p| (p & 0xFF) as u8 == seed_low);
        Liveness::Read {
            seed_low,
            rolling,
            consistent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_sectors_msb_first() {
        let l = Liveness::from_sectors(Some([0xA, 0xB, 0xC, 0xD, 0xE, 0xF]), Some(0x1AB));
        assert_eq!(
            l,
            Liveness::Read {
                seed_low: 0xAB,
                rolling: 0xCDEF,
                consistent: true
            }
        );
        let l = Liveness::from_sectors(Some([0xA, 0xB, 0xC, 0xD, 0xE, 0xF]), Some(0x1AC));
        assert!(matches!(
            l,
            Liveness::Read {
                consistent: false,
                ..
            }
        ));
        assert_eq!(Liveness::from_sectors(None, None), Liveness::Unknown);
    }
}
