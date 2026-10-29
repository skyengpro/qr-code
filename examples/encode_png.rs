//! Payload to a PNG of the first frame.
//!
//! Reads `examples/data/ticket.cbor`, starts a session (profile M, disc) and writes frame 0
//! to `out/encode_png.png`.
//!
//! ```sh
//! cargo run --release -p particlecode-cli --example encode_png
//! ```
#![allow(clippy::print_stdout, clippy::print_stderr)] // examples report to the terminal

use anyhow::{Context, Result};
use particlecode_cli::spec::SessionSpec;
use particlecode_render::{RenderConfig, encode_png, render};

fn main() -> Result<()> {
    let payload = std::fs::read("examples/data/ticket.cbor")
        .context("run this example from the repository root")?;

    // A fixed identity makes the picture reproducible; leave the fields `None` for a random
    // session id and key.
    let spec = SessionSpec {
        session_id: Some(0xC0DE),
        key: Some([0x42; 32]),
        clock_origin: Some(1_780_000_000),
        ..SessionSpec::default()
    };
    let session = spec.build(&payload)?;
    println!(
        "{} payload bytes -> profile {:?}, {} dots, {} bytes per frame, {} frames per pass",
        payload.len(),
        session.profile(),
        session.dot_order().len(),
        session.block_size(),
        session.frames_per_pass()
    );

    let cfg = RenderConfig {
        size_px: 800,
        ..RenderConfig::default()
    };
    let png = encode_png(&render(&session.frame(0), &cfg))?;

    std::fs::create_dir_all("out")?;
    std::fs::write("out/encode_png.png", &png)?;
    println!("wrote out/encode_png.png ({} bytes)", png.len());
    Ok(())
}
