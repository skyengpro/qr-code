//! Liveness verification: checking the rolling value the particles show.

use flutter_rust_bridge::frb;

use particlecode_core::liveness;

/// Whether `rolling` (the 16-bit value read off the particles) is the value for some second
/// within `skew` seconds of `now_unix`, for the session key `key`.
///
/// The key must be exactly 32 bytes (see
/// [`ParticleEncoder::key`](super::encoder::ParticleEncoder::key)); any other length
/// returns `false`. `skew = 0` accepts only the current second.
#[frb(sync)]
pub fn liveness_verify(key: Vec<u8>, rolling: u16, now_unix: u64, skew: u64) -> bool {
    <[u8; 32]>::try_from(key.as_slice())
        .is_ok_and(|key| liveness::verify(&key, rolling, now_unix, skew))
}

#[cfg(test)]
mod tests {
    use super::*;
    use particlecode_core::liveness::rolling_value;

    #[test]
    fn verifies_own_value_and_rejects_bad_keys() {
        let key = vec![3u8; 32];
        let now = 1_750_000_000;
        let r = rolling_value(&[3; 32], now);
        assert!(liveness_verify(key.clone(), r, now, 0));
        assert!(liveness_verify(key.clone(), r, now + 2, 2));
        assert!(!liveness_verify(vec![3; 31], r, now, 0));
        assert!(!liveness_verify(vec![], r, now, 0));
        assert!(!liveness_verify(vec![3; 33], r, now, 0));
    }
}
