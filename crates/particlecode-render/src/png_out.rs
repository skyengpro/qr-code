//! PNG packager.

use tiny_skia::Pixmap;

use crate::error::RenderError;

/// Encode a rendered frame as a PNG.
///
/// Frames are fully opaque, so the file is plain 8-bit RGB (no alpha channel): flat
/// colours stay byte-exact and compress to a few tens of kilobytes. No gamma or colour
/// profile chunk is written; the palette is meant to be shown as raw sRGB values.
pub fn encode_png(pixmap: &Pixmap) -> Result<Vec<u8>, RenderError> {
    let mut rgb = Vec::with_capacity(pixmap.pixels().len() * 3);
    for p in pixmap.pixels() {
        let c = p.demultiply();
        rgb.extend_from_slice(&[c.red(), c.green(), c.blue()]);
    }
    encode_rgb(&rgb, pixmap.width(), pixmap.height())
}

/// Encode packed RGB8 pixels as a PNG.
pub(crate) fn encode_rgb(rgb: &[u8], width: u32, height: u32) -> Result<Vec<u8>, RenderError> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, width, height);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Balanced);
    let mut writer = enc.write_header()?;
    writer.write_image_data(rgb)?;
    writer.finish()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Color;

    #[test]
    fn round_trips_flat_colours() {
        let mut pm = Pixmap::new(8, 6).expect("valid size");
        pm.fill(Color::from_rgba8(246, 244, 239, 255));
        let bytes = encode_png(&pm).expect("encodes");
        let dec = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = dec.read_info().expect("valid png");
        let mut buf = vec![0; reader.output_buffer_size().expect("size")];
        let info = reader.next_frame(&mut buf).expect("frame");
        assert_eq!((info.width, info.height), (8, 6));
        assert_eq!(info.color_type, png::ColorType::Rgb);
        assert_eq!(&buf[..3], &[246, 244, 239]);
    }
}
