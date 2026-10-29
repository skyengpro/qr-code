//! Demo frames of a shape: random symbols on the shape's dot order, rendered by
//! `particlecode-render`. The symbols are noise, so the frame is for looking at (and, in
//! [`crate::validate`], for degrading), not for decoding a payload.

use particlecode_core::frame::FrameSpec;
use particlecode_core::layout::{DotOrder, HEADER_DOTS, dims};
use particlecode_core::profile::Profile;
use particlecode_render::demo::demo_header;
use particlecode_render::synth::Rng;
use particlecode_render::{RenderConfig, RenderError, encode_png, render};

/// How to render a preview.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewOptions {
    /// Lattice density to show.
    pub profile: Profile,
    /// Image side in pixels.
    pub size_px: u32,
    /// Seed of the random symbols.
    pub seed: u64,
}

impl Default for PreviewOptions {
    fn default() -> Self {
        PreviewOptions {
            profile: Profile::M,
            size_px: 900,
            seed: 1,
        }
    }
}

/// A frame whose header announces `shape_id` / `table_version` and whose dots carry random
/// 3-bit symbols on `order`.
pub fn random_frame(
    order: &DotOrder,
    shape_id: u8,
    table_version: u8,
    index: u32,
    rng: &mut Rng,
) -> FrameSpec {
    let mut header = demo_header(order.profile(), index);
    header.shape_id = shape_id;
    header.shape_table_version = table_version & 3;
    let header_symbols: Vec<u8> = (0..HEADER_DOTS).map(|_| rng.below(8) as u8).collect();
    let body_symbols: Vec<u8> = (0..order.len()).map(|_| rng.below(8) as u8).collect();
    let mut sectors = [0u8; dims::PARTICLE_COUNT];
    for s in &mut sectors {
        *s = rng.below(dims::PARTICLE_SECTORS as u64) as u8;
    }
    FrameSpec::assemble(
        index,
        header,
        &header_symbols,
        order,
        &body_symbols,
        sectors,
    )
}

/// Render one preview frame of `order` as PNG bytes.
pub fn preview_png(
    order: &DotOrder,
    shape_id: u8,
    table_version: u8,
    opts: &PreviewOptions,
) -> Result<Vec<u8>, RenderError> {
    let mut rng = Rng::new(opts.seed);
    let frame = random_frame(order, shape_id, table_version, 0, &mut rng);
    let cfg = RenderConfig {
        size_px: opts.size_px,
        ..RenderConfig::default()
    };
    cfg.validate()?;
    encode_png(&render(&frame, &cfg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use particlecode_core::shapes::ShapeTable;

    #[test]
    fn frame_uses_the_shape_dots_and_header_id() {
        let table = ShapeTable::builtin();
        let order = table.dot_order(28, Profile::S).unwrap();
        let frame = random_frame(&order, 28, 0, 0, &mut Rng::new(3));
        assert_eq!(frame.dots.len(), order.len());
        assert_eq!(frame.header.shape_id, 28);
        assert_eq!(frame.anchors.len(), 4);
    }

    #[test]
    fn png_has_the_signature() {
        let table = ShapeTable::builtin();
        let order = table.dot_order(30, Profile::S).unwrap();
        let opts = PreviewOptions {
            profile: Profile::S,
            size_px: 200,
            seed: 1,
        };
        let png = preview_png(&order, 30, 0, &opts).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }
}
