//! A ~6 KB payload as a looping GIF with the breathing mood and particle trails.
//!
//! The loop is exactly one breathing period long (2.5 s, 25 data frames at 10 fps), so the
//! drift wraps seamlessly even though the payload takes only a few frames per pass: the
//! fountain simply keeps emitting fresh packets.
//!
//! ```sh
//! cargo run --release -p particlecode-cli --example animated_gif
//! ```
#![allow(clippy::print_stdout, clippy::print_stderr)] // examples report to the terminal

use anyhow::{Context, Result};
use particlecode_cli::spec::SessionSpec;
use particlecode_core::motion::{BREATH_PERIOD, Mood, MotionConfig};
use particlecode_render::{GifOptions, RenderConfig, write_gif_with};

fn main() -> Result<()> {
    let payload = std::fs::read("examples/data/credential.cbor")
        .context("run this example from the repository root")?;
    let spec = SessionSpec {
        session_id: Some(0xB12E),
        key: Some([0x17; 32]),
        clock_origin: Some(1_780_000_000),
        ..SessionSpec::default()
    };
    let session = spec.build(&payload)?;

    let fps = 10;
    let cfg = RenderConfig {
        size_px: 400,
        fps,
        ..RenderConfig::default()
    };
    let frame_count = (BREATH_PERIOD * f64::from(fps)).round() as u32;
    let frames: Vec<_> = (0..frame_count).map(|i| session.frame(i)).collect();
    println!(
        "{} payload bytes, {} frames per pass, {} frames in the loop ({:.1} s)",
        payload.len(),
        session.frames_per_pass(),
        frames.len(),
        f64::from(frame_count) / f64::from(fps)
    );

    let motion = MotionConfig {
        mood: Mood::Breathing,
        particle_trails: true,
        ..MotionConfig::default()
    };
    // Two rendered GIF frames per data frame bake the drift into the file.
    let opts = GifOptions {
        subframes: 2,
        crossfade: false,
    };
    let gif = write_gif_with(&frames, &cfg, &motion, &opts)?;

    std::fs::create_dir_all("out")?;
    std::fs::write("out/animated_gif.gif", &gif)?;
    println!("wrote out/animated_gif.gif ({} KiB)", gif.len() / 1024);
    Ok(())
}
