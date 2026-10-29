//! JSON form of a frame, for painters that parse text instead of binding DTOs.

use particlecode_core::FrameSpec;

/// Serialize a [`FrameSpec`] with serde.
///
/// The shape is the core type's serde form: `index`, `header`, `profile`,
/// `frame_half_side`, `particle_band`, `anchors`, `swatches`, `header_dots`, `dots`,
/// `particles`. Coordinates are in pitch units, `y` down; colors are indices into the
/// palette (see [`crate::api::dto::palette`]).
pub fn frame_json(spec: &FrameSpec) -> String {
    // `FrameSpec` is plain data with string-keyed structs only, which serde_json never
    // fails on.
    serde_json::to_string(spec).expect("FrameSpec serializes to JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::dto::EncoderConfig;
    use crate::api::encoder::ParticleEncoder;

    #[test]
    fn json_parses_back_to_the_same_frame() {
        let enc = ParticleEncoder::new(b"hello json".to_vec(), EncoderConfig::default()).unwrap();
        let spec = enc.spec(0);
        let back: FrameSpec = serde_json::from_str(&frame_json(&spec)).unwrap();
        assert_eq!(back, spec);
    }
}
