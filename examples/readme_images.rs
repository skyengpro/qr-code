//! Regenerates the pictures the top-level README shows, into `docs/images/`.
//!
//! * `hero.png` - profile M, disc, 640 px
//! * `shapes.png` - contact sheet of shape families
//! * `letter.png` - the letter P as a mask
//! * `animated.gif` - short looping animation (breathing mood, particle trails)
//! * `degraded.png` - the hero frame as a tilted, blurred, noisy camera view
//!
//! ```sh
//! cargo run --release -p particlecode-cli --example readme_images [OUT_DIR]
//! ```
#![allow(clippy::print_stdout, clippy::print_stderr)] // examples report to the terminal

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use particlecode_cli::cli::parse_shape;
use particlecode_cli::sample;
use particlecode_cli::sheet::shape_sheet;
use particlecode_cli::spec::SessionSpec;
use particlecode_core::motion::{BREATH_PERIOD, Mood, MotionConfig};
use particlecode_core::profile::{Profile, Rgb};
use particlecode_core::{Session, ShapeChoice};
use particlecode_render::synth::{CameraPose, Degradation, RgbImage, apply_all};
use particlecode_render::{GifOptions, RenderConfig, encode_png, render, write_gif_with};

fn main() -> Result<()> {
    let dir: PathBuf = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("docs/images"), PathBuf::from);
    std::fs::create_dir_all(&dir)?;
    let payload = sample::credential(6000);

    hero(&dir, &payload)?;
    shapes(&dir, &payload)?;
    letter(&dir, &payload)?;
    animated(&dir, &payload)?;
    degraded(&dir, &payload)?;
    Ok(())
}

fn session(payload: &[u8], shape: u8, session_id: u16) -> Result<Session> {
    SessionSpec {
        profile: Profile::M,
        shape: ShapeChoice::Require(shape),
        session_id: Some(session_id),
        key: Some([0x42; 32]),
        clock_origin: Some(1_780_000_000),
        ..SessionSpec::default()
    }
    .build(payload)
}

fn save(dir: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    let path = dir.join(name);
    std::fs::write(&path, bytes).with_context(|| format!("write {}", path.display()))?;
    println!("{:>14}  {:>5} KiB", name, bytes.len().div_ceil(1024));
    Ok(())
}

fn hero(dir: &Path, payload: &[u8]) -> Result<()> {
    let cfg = RenderConfig {
        size_px: 640,
        ..RenderConfig::default()
    };
    let s = session(payload, 0, 0xA11C)?;
    save(dir, "hero.png", &encode_png(&render(&s.frame(0), &cfg))?)
}

fn shapes(dir: &Path, payload: &[u8]) -> Result<()> {
    let ids = [
        "heart",
        "rounded-square",
        "hexagon",
        "ring",
        "P",
        "A",
        "R",
        "T",
    ]
    .iter()
    .map(|n| parse_shape(n).map_err(|e| anyhow!(e)))
    .collect::<Result<Vec<u8>>>()?;
    let sheet = shape_sheet(&ids, payload, Profile::M, 300, 4, 10)?;
    save(dir, "shapes.png", &encode_png(&sheet)?)
}

fn letter(dir: &Path, payload: &[u8]) -> Result<()> {
    let cfg = RenderConfig {
        size_px: 640,
        ..RenderConfig::default()
    };
    let p = parse_shape("P").map_err(|e| anyhow!(e))?;
    let s = session(payload, p, 0xA11D)?;
    save(dir, "letter.png", &encode_png(&render(&s.frame(0), &cfg))?)
}

fn animated(dir: &Path, payload: &[u8]) -> Result<()> {
    let s = session(payload, 0, 0xA11E)?;
    let fps = 10;
    let cfg = RenderConfig {
        size_px: 360,
        fps,
        ..RenderConfig::default()
    };
    // One breathing period, so the loop is seamless.
    let n = (BREATH_PERIOD * f64::from(fps)).round() as u32;
    let frames: Vec<_> = (0..n).map(|i| s.frame(i)).collect();
    let motion = MotionConfig {
        mood: Mood::Breathing,
        particle_trails: true,
        ..MotionConfig::default()
    };
    let opts = GifOptions {
        subframes: 1,
        crossfade: false,
    };
    save(
        dir,
        "animated.gif",
        &write_gif_with(&frames, &cfg, &motion, &opts)?,
    )
}

fn degraded(dir: &Path, payload: &[u8]) -> Result<()> {
    let cfg = RenderConfig {
        size_px: 900,
        ..RenderConfig::default()
    };
    let s = session(payload, 0, 0xA11C)?;
    let flat = RgbImage::from_pixmap(&render(&s.frame(0), &cfg));
    // A phone held at an angle over a dark desk, in warm light, slightly out of focus.
    let desk = Rgb(58, 52, 48);
    let chain = [
        Degradation::Tilt {
            pose: CameraPose {
                rotate_deg: 9.0,
                tilt_x_deg: 28.0,
                tilt_y_deg: -18.0,
                scale: 0.8,
            },
            out_w: 960,
            out_h: 720,
            background: desk,
        },
        Degradation::ColorCast {
            r: 1.06,
            g: 1.0,
            b: 0.88,
        },
        Degradation::Exposure {
            gain: 0.92,
            offset: -4.0,
        },
        Degradation::Blur { sigma: 1.1 },
        Degradation::Noise {
            sigma: 7.0,
            seed: 11,
        },
        Degradation::Downscale {
            width: 720,
            height: 540,
        },
    ];
    let png = apply_all(&flat, &chain).to_png()?;
    save(dir, "degraded.png", &png)
}
