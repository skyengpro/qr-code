//! Complete demo frames without the fountain coder: a disc of pseudo-random symbols.
//!
//! Handy for looking at the painter, for benchmarks and for vision tests that only need
//! *a* valid-looking frame. The symbols are random, so the body is not decodable payload.

use particlecode_core::frame::FrameSpec;
use particlecode_core::header::Header;
use particlecode_core::layout::{DotOrder, HEADER_DOTS, Lattice, dims};
use particlecode_core::profile::{Palette, Profile};

use crate::synth::Rng;

/// A header with plausible values for `profile`; `packet_seed` is `index mod 4096`.
pub fn demo_header(profile: Profile, index: u32) -> Header {
    Header {
        shape_table_version: 0,
        profile,
        palette: Palette::Hues4,
        is_static: false,
        shape_id: 0,
        session_id: 0x5A17,
        payload_len: 3000,
        segment_kb_log2: 2,
        segment_index: (index / 8) as u16 % 1024,
        packet_seed: (index % 4096) as u16,
    }
}

/// The disc mask over the profile's lattice (`|p| <= F`) as a [`DotOrder`].
pub fn disc_order(profile: Profile) -> DotOrder {
    let lattice = Lattice::new(profile);
    let f = profile.field_half();
    let mask: Vec<bool> = lattice.points().iter().map(|p| p.at.norm() <= f).collect();
    DotOrder::new(&lattice, &mask)
}

/// A full frame: disc mask, uniformly random 3-bit symbols for the 48 header dots and the
/// body, random particle sectors. Deterministic in `(profile, seed, index)`.
pub fn demo_frame(profile: Profile, seed: u64, index: u32) -> FrameSpec {
    let mut rng = Rng::new(seed ^ (u64::from(index) << 32) ^ 0xD3A0);
    let order = disc_order(profile);
    let header_symbols: Vec<u8> = (0..HEADER_DOTS).map(|_| rng.below(8) as u8).collect();
    let body_symbols: Vec<u8> = (0..order.len()).map(|_| rng.below(8) as u8).collect();
    let mut sectors = [0u8; dims::PARTICLE_COUNT];
    for s in &mut sectors {
        *s = rng.below(dims::PARTICLE_SECTORS as u64) as u8;
    }
    FrameSpec::assemble(
        index,
        demo_header(profile, index),
        &header_symbols,
        &order,
        &body_symbols,
        sectors,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_deterministic_and_complete() {
        let a = demo_frame(Profile::S, 1, 0);
        assert_eq!(a, demo_frame(Profile::S, 1, 0));
        assert_ne!(a, demo_frame(Profile::S, 2, 0));
        assert_eq!(a.header_dots.len(), 48);
        assert_eq!(a.swatches.len(), 48);
        assert_eq!(a.particles.len(), 6);
        assert_eq!(a.anchors.len(), 4);
        assert!(a.dots.len() > 1500);
    }
}
