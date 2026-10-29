//! One frame per built-in shape family on a single contact sheet.
//!
//! Shows the disc, rounded square, heart, hexagon, ring and the letters P, A, R, T, each
//! rendered at 360 px and blitted into a 3 x 3 grid: `out/shapes_gallery.png`.
//!
//! ```sh
//! cargo run --release -p particlecode-cli --example shapes_gallery
//! ```
#![allow(clippy::print_stdout, clippy::print_stderr)] // examples report to the terminal

use anyhow::{Context, Result, anyhow};
use particlecode_cli::cli::parse_shape;
use particlecode_cli::sheet::shape_sheet;
use particlecode_core::Profile;
use particlecode_render::encode_png;

fn main() -> Result<()> {
    let payload = std::fs::read("examples/data/ticket.cbor")
        .context("run this example from the repository root")?;

    let names = [
        "disc",
        "rounded-square",
        "heart",
        "hexagon",
        "ring",
        "P",
        "A",
        "R",
        "T",
    ];
    let ids = names
        .iter()
        .map(|n| parse_shape(n).map_err(|e| anyhow!(e)))
        .collect::<Result<Vec<u8>>>()?;
    for (name, id) in names.iter().zip(&ids) {
        println!("{name:>15} = shape id {id}");
    }

    let sheet = shape_sheet(&ids, &payload, Profile::M, 360, 3, 12)?;
    let png = encode_png(&sheet)?;
    std::fs::create_dir_all("out")?;
    std::fs::write("out/shapes_gallery.png", &png)?;
    println!(
        "wrote out/shapes_gallery.png ({} x {} px, {} KiB)",
        sheet.width(),
        sheet.height(),
        png.len() / 1024
    );
    Ok(())
}
