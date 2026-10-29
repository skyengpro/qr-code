//! A static, high-ECC frame for print: vector SVG plus a 600 dpi PNG.
//!
//! The payload must fit one frame (`--static` in the CLI). Both files are sized for a dot
//! pitch of 1.3 mm on paper (the spec asks for at least 1.2 mm). The SVG declares its size
//! in millimetres and scales losslessly; the PNG carries 600 dpi metadata.
//!
//! ```sh
//! cargo run --release -p particlecode-cli --example print_poster
//! ```
#![allow(clippy::print_stdout, clippy::print_stderr)] // examples report to the terminal

use anyhow::{Context, Result};
use particlecode_cli::sample;
use particlecode_cli::spec::SessionSpec;
use particlecode_render::{RenderConfig, render, render_svg};
use tiny_skia::Pixmap;

/// Dot pitch on paper in millimetres.
const PITCH_MM: f64 = 1.3;
/// Print resolution.
const DPI: f64 = 600.0;
const MM_PER_INCH: f64 = 25.4;

fn main() -> Result<()> {
    let payload = sample::credential(800);
    let spec = SessionSpec {
        is_static: true,
        session_id: Some(0x0057),
        key: Some([0x99; 32]),
        clock_origin: Some(1_780_000_000),
        ..SessionSpec::default()
    };
    let session = spec.build(&payload)?;
    let frame = session.frame(0);
    println!(
        "{} payload bytes in one frame (capacity {} bytes at high ECC), {} dots",
        payload.len(),
        session.block_size(),
        session.dot_order().len()
    );

    // Pixels per pitch at 600 dpi, then the image side that makes the pitch exactly 1.3 mm.
    let px_per_pitch = PITCH_MM / MM_PER_INCH * DPI;
    let probe = RenderConfig::default();
    let side_pitches = 2.0 * probe.half_extent(&frame);
    let size_px = (side_pitches * px_per_pitch).round() as u32;
    let side_mm = side_pitches * PITCH_MM;
    let cfg = RenderConfig { size_px, ..probe };
    println!("{side_mm:.1} mm square, {size_px} px at {DPI} dpi");

    std::fs::create_dir_all("out")?;

    // SVG: swap the pixel size for millimetres so every viewer prints it at the right size.
    let svg = render_svg(&frame, &cfg);
    let sized = svg.replacen(
        &format!("width=\"{size_px}\" height=\"{size_px}\""),
        &format!("width=\"{side_mm:.2}mm\" height=\"{side_mm:.2}mm\""),
        1,
    );
    std::fs::write("out/print_poster.svg", &sized)?;
    println!("wrote out/print_poster.svg ({} KiB)", sized.len() / 1024);

    let png = png_with_dpi(&render(&frame, &cfg), DPI)?;
    std::fs::write("out/print_poster.png", &png)?;
    println!("wrote out/print_poster.png ({} KiB)", png.len() / 1024);
    Ok(())
}

/// Encode an opaque pixmap as RGB PNG with a `pHYs` chunk for `dpi`.
fn png_with_dpi(pixmap: &Pixmap, dpi: f64) -> Result<Vec<u8>> {
    let rgb: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue()]
        })
        .collect();
    let per_metre = (dpi / MM_PER_INCH * 1000.0).round() as u32;
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, pixmap.width(), pixmap.height());
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_pixel_dims(Some(png::PixelDimensions {
        xppu: per_metre,
        yppu: per_metre,
        unit: png::Unit::Meter,
    }));
    let mut writer = enc.write_header().context("PNG header")?;
    writer.write_image_data(&rgb).context("PNG pixels")?;
    writer.finish().context("PNG end")?;
    Ok(out)
}
