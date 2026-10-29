//! `encode`: payload file to PNG, GIF, SVG or a PNG sequence.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use particlecode_core::motion::MotionConfig;
use particlecode_core::shapes::ShapeTable;
use particlecode_core::{Profile, Session, ShapeChoice};
use particlecode_render::{Manifest, RenderConfig, encode_png, render, render_svg, write_gif};

use crate::cli::{EncodeArgs, FormatArg, to_hex};
use crate::spec::SessionSpec;

/// Most frames one `gif` or `seq` run may produce; larger payloads need bigger segments or
/// fewer passes.
pub const MAX_FRAMES: u32 = 20_000;

/// What an `encode` run produced, printed to stderr.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// Lattice profile.
    pub profile: Profile,
    /// Shape name.
    pub shape: String,
    /// Payload dots per frame.
    pub dots: usize,
    /// Payload bytes carried per frame.
    pub bytes_per_frame: usize,
    /// Frames in one pass.
    pub frames_per_pass: u32,
    /// Payload segments.
    pub segments: usize,
    /// Session id.
    pub session_id: u16,
    /// Session key, lowercase hex.
    pub key_hex: String,
    /// Frames written.
    pub frames_written: u32,
    /// Where the output went.
    pub output: PathBuf,
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "profile {:?}, shape {}, {} dots, {} bytes/frame",
            self.profile, self.shape, self.dots, self.bytes_per_frame
        )?;
        writeln!(
            f,
            "{} frame(s) per pass, {} segment(s)",
            self.frames_per_pass, self.segments
        )?;
        writeln!(
            f,
            "session id {:04x}, key {}",
            self.session_id, self.key_hex
        )?;
        write!(
            f,
            "wrote {} frame(s) to {}",
            self.frames_written,
            self.output.display()
        )
    }
}

/// The format to use: explicit, else from the extension of `output`, else a sequence for a
/// path without extension.
pub fn resolve_format(explicit: Option<FormatArg>, output: &Path) -> Result<FormatArg> {
    if let Some(f) = explicit {
        return Ok(f);
    }
    match output.extension().and_then(|e| e.to_str()) {
        None => Ok(FormatArg::Seq),
        Some(e) => match e.to_ascii_lowercase().as_str() {
            "png" => Ok(FormatArg::Png),
            "gif" => Ok(FormatArg::Gif),
            "svg" => Ok(FormatArg::Svg),
            other => bail!("cannot infer the format from the extension .{other}; pass --format"),
        },
    }
}

/// The shape selection implied by `--shape` / `--prefer-shape`.
pub fn shape_choice(required: Option<u8>, preferred: &[u8]) -> ShapeChoice {
    match (required, preferred) {
        (Some(id), _) => ShapeChoice::Require(id),
        (None, []) => ShapeChoice::Require(0),
        (None, [id]) => ShapeChoice::Prefer(*id),
        (None, ids) => ShapeChoice::Auto(ids.to_vec()),
    }
}

/// Encode `payload` as `args` describes and write the output.
pub fn run(args: &EncodeArgs, payload: &[u8]) -> Result<Summary> {
    let format = resolve_format(args.format, &args.output)?;
    let spec = SessionSpec {
        profile: args.profile.into(),
        shape: shape_choice(args.shape, &args.prefer_shape),
        is_static: args.is_static,
        segment_kb_log2: args.segment_kb_log2,
        fps: args.fps,
        session_id: args.session_id,
        key: args.key,
        clock_origin: args.clock_origin,
    };
    let session = spec.build(payload)?;
    let default_size = match format {
        FormatArg::Png | FormatArg::Svg => 800,
        FormatArg::Seq => 640,
        FormatArg::Gif => 480,
    };
    let cfg = RenderConfig {
        size_px: args.size.unwrap_or(default_size),
        fps: u32::from(args.fps),
        ..RenderConfig::default()
    };
    cfg.validate().context("invalid image size")?;

    let frames_written = match format {
        FormatArg::Png => {
            let png = encode_png(&render(&session.frame(args.frame), &cfg))?;
            write_file(&args.output, &png)?;
            1
        }
        FormatArg::Svg => {
            let svg = render_svg(&session.frame(args.frame), &cfg);
            write_file(&args.output, svg.as_bytes())?;
            1
        }
        FormatArg::Gif => {
            let frames = collect_frames(&session, args.passes, args.is_static)?;
            let motion = motion_config(args);
            let gif = write_gif(&frames, &cfg, &motion)?;
            write_file(&args.output, &gif)?;
            frame_count(&frames)
        }
        FormatArg::Seq => {
            let frames = collect_frames(&session, args.passes, args.is_static)?;
            let manifest = Manifest::from_frames(&frames, &cfg)?;
            particlecode_render::write_sequence(&args.output, &frames, &cfg, &manifest)?;
            frame_count(&frames)
        }
    };

    let table = ShapeTable::builtin();
    let shape_id = session.packets().packet(0).0.shape_id;
    Ok(Summary {
        profile: session.profile(),
        shape: table
            .name(shape_id)
            .map_or_else(|| format!("#{shape_id}"), str::to_owned),
        dots: session.dot_order().len(),
        bytes_per_frame: session.block_size(),
        frames_per_pass: session.frames_per_pass(),
        segments: session.packets().segment_count(),
        session_id: session.session_id(),
        key_hex: to_hex(session.key()),
        frames_written,
        output: args.output.clone(),
    })
}

fn frame_count<T>(frames: &[T]) -> u32 {
    u32::try_from(frames.len()).unwrap_or(u32::MAX)
}

/// Motion layers for gif output.
fn motion_config(args: &EncodeArgs) -> MotionConfig {
    let base = if args.all_moods {
        MotionConfig::all()
    } else {
        MotionConfig::default()
    };
    MotionConfig {
        mood: args.mood.into(),
        ..base
    }
}

/// `passes` passes of the session (one frame for a static session).
fn collect_frames(
    session: &Session,
    passes: u32,
    is_static: bool,
) -> Result<Vec<particlecode_core::FrameSpec>> {
    let per_pass = session.frames_per_pass();
    let count = if is_static {
        1
    } else {
        per_pass.saturating_mul(passes)
    };
    if count > MAX_FRAMES {
        bail!(
            "{count} frames requested, more than the limit of {MAX_FRAMES}; use fewer passes, a \
             denser profile or a larger --segment-kb-log2"
        );
    }
    Ok((0..count).map(|i| session.frame(i)).collect())
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    fs::write(path, bytes).with_context(|| format!("cannot write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Command};
    use clap::Parser as _;

    fn args(extra: &[&str]) -> EncodeArgs {
        let cli = Cli::try_parse_from(
            ["particlecode", "encode", "in", "-o", "x.png"]
                .into_iter()
                .chain(extra.iter().copied()),
        )
        .expect("parses");
        match cli.command {
            Command::Encode(a) => a,
            _ => panic!("not encode"),
        }
    }

    #[test]
    fn format_inference() {
        let p = |s: &str| resolve_format(None, Path::new(s)).map_err(|e| e.to_string());
        assert_eq!(p("a.png"), Ok(FormatArg::Png));
        assert_eq!(p("a.GIF"), Ok(FormatArg::Gif));
        assert_eq!(p("dir/a.svg"), Ok(FormatArg::Svg));
        assert_eq!(p("frames"), Ok(FormatArg::Seq));
        assert!(p("a.jpg").is_err());
        assert_eq!(
            resolve_format(Some(FormatArg::Gif), Path::new("a.png")).ok(),
            Some(FormatArg::Gif)
        );
    }

    #[test]
    fn shape_selection() {
        assert_eq!(shape_choice(None, &[]), ShapeChoice::Require(0));
        assert_eq!(shape_choice(Some(28), &[]), ShapeChoice::Require(28));
        assert_eq!(shape_choice(None, &[28]), ShapeChoice::Prefer(28));
        assert_eq!(
            shape_choice(None, &[28, 17]),
            ShapeChoice::Auto(vec![28, 17])
        );
    }

    #[test]
    fn motion_layers() {
        let m = motion_config(&args(&["--mood", "flow"]));
        assert_eq!(m.mood, particlecode_core::motion::Mood::FlowField);
        assert!(!m.ripple);
        let m = motion_config(&args(&["--all-moods"]));
        assert!(m.ripple && m.particle_trails && m.reveal);
    }

    #[test]
    fn frame_limit_is_enforced() {
        let session = SessionSpec::default().build(&[1u8; 300]).expect("session");
        assert!(collect_frames(&session, 1, false).is_ok());
        assert_eq!(collect_frames(&session, 2, true).expect("static").len(), 1);
        let big = SessionSpec::default()
            .build(&vec![3u8; 400_000])
            .expect("session");
        assert!(collect_frames(&big, 64, false).is_err());
    }

    #[test]
    fn summary_text_mentions_the_essentials() {
        let s = Summary {
            profile: Profile::M,
            shape: "disc".into(),
            dots: 3800,
            bytes_per_frame: 1000,
            frames_per_pass: 3,
            segments: 1,
            session_id: 0xAB,
            key_hex: "00".repeat(32),
            frames_written: 1,
            output: PathBuf::from("out.png"),
        };
        let text = s.to_string();
        assert!(text.contains("disc") && text.contains("00ab") && text.contains("out.png"));
    }
}
