//! The `particlecode-shapes` command line: argument definitions and subcommands.
//!
//! [`run`] writes everything it reports to the writer it is given, so the subcommands are
//! testable without capturing stdout.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Parser, Subcommand};
use particlecode_core::layout::{DotOrder, Lattice};
use particlecode_core::profile::{EccLevel, Profile};
use particlecode_core::shapes::{FIRST_CUSTOM, ShapeTable};

use crate::build::{BuildError, add_shape};
use crate::path::ViewBox;
use crate::preview::{PreviewOptions, preview_png};
use crate::raster::ShapeInput;
use crate::report::{profile_name, to_json, to_text, validate_json, validate_text};
use crate::rules::check;
use crate::validate::{self, SyntheticFrames, ValidateConfig, compare_with_disc};

/// Tool for custom Particle Code shapes: SVG path to lattice bitmaps, rule checks, previews.
#[derive(Debug, Parser)]
#[command(name = "particlecode-shapes", version, about)]
pub struct Cli {
    /// What to do.
    #[command(subcommand)]
    pub command: Command,
}

/// Subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// List the shapes of a table with their capacities.
    List(ListArgs),
    /// Check an SVG path against the shape rules (spec rules 2-6).
    Check(CheckArgs),
    /// Add checked SVG shapes to a table and write `shapes.bin`.
    Build(BuildArgs),
    /// Render a demo frame (random symbols, anchors) with a shape's mask.
    Preview(PreviewArgs),
    /// Write a PNG preview of every built-in shape.
    ExportBuiltins(ExportArgs),
    /// Rule 7: synthetic frames through a camera model, symbol error rate against the disc.
    Validate(ValidateArgs),
}

/// `list`.
#[derive(Debug, Args)]
pub struct ListArgs {
    /// A `shapes.bin` whose custom shapes are listed after the built-ins.
    #[arg(long)]
    pub table: Option<PathBuf>,
}

/// `check`.
#[derive(Debug, Args)]
pub struct CheckArgs {
    /// An `.svg` file (first `<path d="...">`, optional square `viewBox`).
    #[arg(conflicts_with_all = ["d", "id"], required_unless_present_any = ["d", "id"])]
    pub svg: Option<PathBuf>,
    /// SVG path data in the 1000 x 1000 box, instead of a file.
    #[arg(long)]
    pub d: Option<String>,
    /// Check an existing shape of the built-ins or of `--table` instead.
    #[arg(long)]
    pub id: Option<u8>,
    /// Table to look `--id` up in.
    #[arg(long, requires = "id")]
    pub table: Option<PathBuf>,
    /// Stroke the path with this width (design units, round caps and joins) and use the
    /// outline as the ink.
    #[arg(long)]
    pub stroke: Option<f32>,
    /// Print the report as JSON.
    #[arg(long)]
    pub json: bool,
}

/// `build`.
#[derive(Debug, Args)]
pub struct BuildArgs {
    /// `.svg` files to add; the first gets `--id`, the next `--id + 1`, and so on.
    #[arg(required = true)]
    pub svg: Vec<PathBuf>,
    /// Shape id of the first file (32..=255).
    #[arg(long)]
    pub id: u8,
    /// Shape name (single file only; default: the file name without extension).
    #[arg(long)]
    pub name: Option<String>,
    /// Table to extend (default: a table with the built-ins only).
    #[arg(long)]
    pub table: Option<PathBuf>,
    /// Stroke every path with this width before filling.
    #[arg(long)]
    pub stroke: Option<f32>,
    /// Add shapes that fail the rules.
    #[arg(long)]
    pub force: bool,
    /// Increase the table version by one (it travels in the frame header; max 3).
    #[arg(long)]
    pub bump_version: bool,
    /// Output `shapes.bin`.
    #[arg(short, long)]
    pub output: PathBuf,
}

/// `preview`.
#[derive(Debug, Args)]
pub struct PreviewArgs {
    /// A shape id (built-in or from `--table`) or an `.svg` file.
    pub shape: String,
    /// Table to look the id up in.
    #[arg(long)]
    pub table: Option<PathBuf>,
    /// Stroke an SVG path with this width first.
    #[arg(long)]
    pub stroke: Option<f32>,
    /// Profile to show: s, m or l.
    #[arg(long, default_value = "m", value_parser = parse_profile)]
    pub profile: Profile,
    /// Image side in pixels.
    #[arg(long, default_value_t = 900)]
    pub size: u32,
    /// Seed of the random symbols.
    #[arg(long, default_value_t = 1)]
    pub seed: u64,
    /// Output PNG.
    #[arg(short, long)]
    pub output: PathBuf,
}

/// `export-builtins`.
#[derive(Debug, Args)]
pub struct ExportArgs {
    /// Output directory (created if missing).
    #[arg(short, long)]
    pub output: PathBuf,
    /// Profile to show: s, m or l.
    #[arg(long, default_value = "m", value_parser = parse_profile)]
    pub profile: Profile,
    /// Image side in pixels.
    #[arg(long, default_value_t = 600)]
    pub size: u32,
}

/// `validate`.
#[derive(Debug, Args)]
pub struct ValidateArgs {
    /// A shape id or an `.svg` file.
    pub shape: String,
    /// Table to look the id up in.
    #[arg(long)]
    pub table: Option<PathBuf>,
    /// Stroke an SVG path with this width first.
    #[arg(long)]
    pub stroke: Option<f32>,
    /// Profile: s, m or l.
    #[arg(long, default_value = "m", value_parser = parse_profile)]
    pub profile: Profile,
    /// Number of synthetic frames.
    #[arg(long, default_value_t = validate::DEFAULT_FRAMES)]
    pub frames: usize,
    /// Seed of symbols and degradations.
    #[arg(long, default_value_t = 7)]
    pub seed: u64,
    /// Write the first synthetic camera images here as PNG.
    #[arg(long)]
    pub dump: Option<PathBuf>,
    /// How many images `--dump` writes.
    #[arg(long, default_value_t = 4)]
    pub dump_count: usize,
    /// Worker threads (default: one per core).
    #[arg(long, default_value_t = 0)]
    pub threads: usize,
    /// Print the result as JSON.
    #[arg(long)]
    pub json: bool,
    /// No progress lines on stderr.
    #[arg(long)]
    pub quiet: bool,
}

/// How a command ended, mapped to the process exit code by `main`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Done; rules (if any) hold. Exit code 0.
    Success,
    /// The shape fails rules 2-6, or rule 7 for `validate`. Exit code 1.
    RulesFailed,
}

fn parse_profile(s: &str) -> Result<Profile, String> {
    match s.to_ascii_lowercase().as_str() {
        "s" => Ok(Profile::S),
        "m" => Ok(Profile::M),
        "l" => Ok(Profile::L),
        _ => Err(format!("profile {s:?} is not one of s, m, l")),
    }
}

/// Execute `cli`, writing reports to `out`.
pub fn run(cli: Cli, out: &mut dyn Write) -> Result<Outcome> {
    match cli.command {
        Command::List(a) => list(&a, out),
        Command::Check(a) => check_cmd(&a, out),
        Command::Build(a) => build_cmd(&a, out),
        Command::Preview(a) => preview_cmd(&a, out),
        Command::ExportBuiltins(a) => export_cmd(&a, out),
        Command::Validate(a) => validate_cmd(&a, out),
    }
}

fn load_table(path: Option<&Path>) -> Result<ShapeTable> {
    match path {
        None => Ok(ShapeTable::builtin()),
        Some(p) => {
            let bytes = fs::read(p).with_context(|| format!("reading {}", p.display()))?;
            ShapeTable::from_bytes(&bytes).with_context(|| format!("parsing {}", p.display()))
        }
    }
}

fn load_svg(path: &Path, stroke: Option<f32>) -> Result<ShapeInput> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let label = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    ShapeInput::from_svg(&text, stroke, &label).with_context(|| format!("{}", path.display()))
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "shape".to_string(), |s| s.to_string_lossy().into_owned())
}

fn list(args: &ListArgs, out: &mut dyn Write) -> Result<Outcome> {
    let table = load_table(args.table.as_deref())?;
    let ids: Vec<u8> = table.ids().collect();
    writeln!(
        out,
        "shape table version {}, {} shapes ({} custom)",
        table.version(),
        ids.len(),
        ids.iter().filter(|&&id| id >= FIRST_CUSTOM).count()
    )?;
    writeln!(
        out,
        " id  name              usable dots S / M / L       bytes per frame, Normal ECC S / M / L   High ECC M"
    )?;
    for id in ids {
        let name = table.name(id).unwrap_or("?");
        let mut dots = [0usize; 3];
        let mut normal = [0usize; 3];
        let mut high_m = 0;
        for (i, profile) in Profile::ALL.into_iter().enumerate() {
            let n = table.dot_order(id, profile).map_or(0, |o| o.len());
            dots[i] = n;
            normal[i] = particlecode_core::body::BodyCodec::new(n, EccLevel::Normal).block_size();
            if profile == Profile::M {
                high_m = particlecode_core::body::BodyCodec::new(n, EccLevel::High).block_size();
            }
        }
        writeln!(
            out,
            "{id:>3}  {name:<16}  {:>5} /{:>5} /{:>5}            {:>5} /{:>5} /{:>5}                     {high_m:>5}",
            dots[0], dots[1], dots[2], normal[0], normal[1], normal[2]
        )?;
    }
    Ok(Outcome::Success)
}

fn check_cmd(args: &CheckArgs, out: &mut dyn Write) -> Result<Outcome> {
    let input = if let Some(d) = &args.d {
        ShapeInput::from_path_data(d, &ViewBox::DESIGN, args.stroke, "--d path")?
    } else if let Some(id) = args.id {
        let table = load_table(args.table.as_deref())?;
        ShapeInput::from_table(&table, id).ok_or_else(|| anyhow!("no shape with id {id}"))?
    } else if let Some(svg) = &args.svg {
        load_svg(svg, args.stroke)?
    } else {
        bail!("give an .svg file, --d or --id");
    };
    let report = check(&input);
    if args.json {
        writeln!(out, "{}", to_json(&report))?;
    } else {
        write!(out, "{}", to_text(&report))?;
    }
    Ok(if report.passed() {
        Outcome::Success
    } else {
        Outcome::RulesFailed
    })
}

fn build_cmd(args: &BuildArgs, out: &mut dyn Write) -> Result<Outcome> {
    if args.svg.len() > 1 && args.name.is_some() {
        bail!("--name only makes sense with a single svg file");
    }
    let mut table = load_table(args.table.as_deref())?;
    let mut all_passed = true;
    for (offset, svg) in args.svg.iter().enumerate() {
        let id = u8::try_from(usize::from(args.id) + offset)
            .map_err(|_| anyhow!("shape ids run past 255"))?;
        let name = args.name.clone().unwrap_or_else(|| file_stem(svg));
        let input = load_svg(svg, args.stroke)?;
        match add_shape(&mut table, id, &name, &input, args.force) {
            Ok(report) => {
                write!(out, "{}", to_text(&report))?;
                all_passed &= report.passed();
                writeln!(out, "added shape {id} \"{name}\"")?;
            }
            Err(BuildError::RulesFailed(report)) => {
                write!(out, "{}", to_text(&report))?;
                bail!(
                    "{}: the shape fails the rules above; fix it or pass --force",
                    svg.display()
                );
            }
            Err(e) => bail!("{}: {e}", svg.display()),
        }
    }
    if args.bump_version {
        let next = table.version() + 1;
        table
            .set_version(next)
            .map_err(|e| anyhow!("cannot bump the table version: {e}"))?;
        writeln!(out, "table version bumped to {next}")?;
    }
    fs::write(&args.output, table.to_bytes())
        .with_context(|| format!("writing {}", args.output.display()))?;
    writeln!(
        out,
        "wrote {} (table version {}, {} shapes)",
        args.output.display(),
        table.version(),
        table.ids().count()
    )?;
    Ok(if all_passed {
        Outcome::Success
    } else {
        Outcome::RulesFailed
    })
}

/// A shape to preview or validate: its dot order and how to announce it in the header.
struct Resolved {
    label: String,
    id: u8,
    table_version: u8,
    masks: [Vec<bool>; 3],
}

fn resolve_shape(spec: &str, table_path: Option<&Path>, stroke: Option<f32>) -> Result<Resolved> {
    let path = Path::new(spec);
    if path.exists() {
        let input = load_svg(path, stroke)?;
        return Ok(Resolved {
            label: input.label,
            id: u8::MAX,
            table_version: 0,
            masks: input.masks,
        });
    }
    let id: u8 = spec.parse().map_err(|_| {
        anyhow!("{spec:?} is neither an existing .svg file nor a shape id (0..=255)")
    })?;
    let table = load_table(table_path)?;
    let masks = Profile::ALL.map(|p| table.mask(id, p));
    let [Some(s), Some(m), Some(l)] = masks else {
        bail!("no shape with id {id} in the table");
    };
    Ok(Resolved {
        label: format!("{} (id {id})", table.name(id).unwrap_or("?")),
        id,
        table_version: table.version(),
        masks: [s, m, l],
    })
}

fn order_of(r: &Resolved, profile: Profile) -> DotOrder {
    let idx = Profile::ALL
        .iter()
        .position(|&p| p == profile)
        .unwrap_or_default();
    DotOrder::new(&Lattice::new(profile), &r.masks[idx])
}

fn preview_cmd(args: &PreviewArgs, out: &mut dyn Write) -> Result<Outcome> {
    let shape = resolve_shape(&args.shape, args.table.as_deref(), args.stroke)?;
    let order = order_of(&shape, args.profile);
    let opts = PreviewOptions {
        profile: args.profile,
        size_px: args.size,
        seed: args.seed,
    };
    let png = preview_png(&order, shape.id, shape.table_version, &opts)?;
    fs::write(&args.output, png).with_context(|| format!("writing {}", args.output.display()))?;
    writeln!(
        out,
        "wrote {}: {}, profile {}, {} payload dots",
        args.output.display(),
        shape.label,
        profile_name(args.profile),
        order.len()
    )?;
    Ok(Outcome::Success)
}

fn export_cmd(args: &ExportArgs, out: &mut dyn Write) -> Result<Outcome> {
    fs::create_dir_all(&args.output)
        .with_context(|| format!("creating {}", args.output.display()))?;
    let table = ShapeTable::builtin();
    let opts = PreviewOptions {
        profile: args.profile,
        size_px: args.size,
        seed: 1,
    };
    let mut written = 0;
    for id in table.ids() {
        let Some(order) = table.dot_order(id, args.profile) else {
            continue;
        };
        let name = table.name(id).unwrap_or("shape");
        let file = args.output.join(format!("{id:02}-{name}.png"));
        let png = preview_png(&order, id, table.version(), &opts)?;
        fs::write(&file, png).with_context(|| format!("writing {}", file.display()))?;
        written += 1;
    }
    writeln!(out, "wrote {written} previews to {}", args.output.display())?;
    Ok(Outcome::Success)
}

fn validate_cmd(args: &ValidateArgs, out: &mut dyn Write) -> Result<Outcome> {
    let shape = resolve_shape(&args.shape, args.table.as_deref(), args.stroke)?;
    let order = order_of(&shape, args.profile);
    let table = load_table(args.table.as_deref())?;
    let cfg = ValidateConfig {
        profile: args.profile,
        frames: args.frames,
        seed: args.seed,
        threads: args.threads,
        ..ValidateConfig::default()
    };
    if let Some(dir) = &args.dump {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        let frames = SyntheticFrames::new(&order, shape.id, shape.table_version, cfg.clone());
        for frame in frames.take(args.dump_count) {
            let file = dir.join(format!("synthetic_{:04}.png", frame.index));
            fs::write(&file, frame.image.to_png()?)
                .with_context(|| format!("writing {}", file.display()))?;
        }
        if !args.json {
            writeln!(out, "wrote synthetic camera images to {}", dir.display())?;
        }
    }
    let step = (args.frames / 10).max(1);
    let progress = |phase: &str, done: usize, total: usize| {
        if !args.quiet && (done.is_multiple_of(step) || done == total) {
            let _ = writeln!(std::io::stderr(), "  {phase}: {done}/{total} frames");
        }
    };
    let comparison = compare_with_disc(&order, shape.id, &table, &cfg, &progress)?;
    if args.json {
        writeln!(
            out,
            "{}",
            validate_json(&shape.label, args.profile, &cfg, &comparison)
        )?;
    } else {
        write!(
            out,
            "{}",
            validate_text(&shape.label, args.profile, &cfg, &comparison)
        )?;
    }
    Ok(if comparison.verdict.passed {
        Outcome::Success
    } else {
        Outcome::RulesFailed
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pcshapes-{}-{name}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn run_args(args: &[&str]) -> (Result<Outcome>, String) {
        let cli =
            Cli::try_parse_from(std::iter::once("particlecode-shapes").chain(args.iter().copied()))
                .expect("arguments parse");
        let mut out = Vec::new();
        let result = run(cli, &mut out);
        (result, String::from_utf8(out).unwrap())
    }

    #[test]
    fn list_shows_builtins() {
        let (res, text) = run_args(&["list"]);
        assert_eq!(res.unwrap(), Outcome::Success);
        assert!(text.contains("disc") && text.contains("heart") && text.contains("ring"));
        assert!(text.contains("31 shapes (0 custom)"), "{text}");
    }

    #[test]
    fn check_d_reports_and_sets_exit_outcome() {
        let (res, text) = run_args(&["check", "--d", "M 100 100 L 900 100 L 900 900 L 100 900 Z"]);
        assert_eq!(res.unwrap(), Outcome::Success, "{text}");
        let (res, text) = run_args(&[
            "check",
            "--d",
            "M 450 450 L 550 450 L 550 550 L 450 550 Z",
            "--json",
        ]);
        assert_eq!(res.unwrap(), Outcome::RulesFailed);
        assert!(text.contains("\"passed\":false"), "{text}");
    }

    #[test]
    fn build_refuses_id_5() {
        let dir = scratch("id5");
        let svg = dir.join("sq.svg");
        fs::write(&svg, r#"<svg viewBox="0 0 1000 1000"><path d="M 100 100 L 900 100 L 900 900 L 100 900 Z"/></svg>"#).unwrap();
        let bin = dir.join("shapes.bin");
        let (res, _) = run_args(&[
            "build",
            svg.to_str().unwrap(),
            "--id",
            "5",
            "--force",
            "-o",
            bin.to_str().unwrap(),
        ]);
        let err = res.unwrap_err().to_string();
        assert!(err.contains("reserved"), "{err}");
        assert!(!bin.exists());
    }

    #[test]
    fn build_list_preview_roundtrip() {
        let dir = scratch("roundtrip");
        let svg = dir.join("sq.svg");
        fs::write(&svg, r#"<svg viewBox="0 0 1000 1000"><path d="M 150 150 L 850 150 L 850 850 L 150 850 Z"/></svg>"#).unwrap();
        let bin = dir.join("shapes.bin");
        let (res, text) = run_args(&[
            "build",
            svg.to_str().unwrap(),
            "--id",
            "40",
            "--name",
            "box",
            "--bump-version",
            "-o",
            bin.to_str().unwrap(),
        ]);
        assert_eq!(res.unwrap(), Outcome::Success, "{text}");
        let table = ShapeTable::from_bytes(&fs::read(&bin).unwrap()).unwrap();
        assert_eq!(table.name(40), Some("box"));
        assert_eq!(table.version(), 1);

        let (_, listing) = run_args(&["list", "--table", bin.to_str().unwrap()]);
        assert!(listing.contains(" 40  box"), "{listing}");

        let png = dir.join("p.png");
        let (res, _) = run_args(&[
            "preview",
            "40",
            "--table",
            bin.to_str().unwrap(),
            "--profile",
            "s",
            "--size",
            "300",
            "-o",
            png.to_str().unwrap(),
        ]);
        assert_eq!(res.unwrap(), Outcome::Success);
        assert_eq!(&fs::read(&png).unwrap()[..4], b"\x89PNG");
        // Re-using the id is refused.
        let (res, _) = run_args(&[
            "build",
            svg.to_str().unwrap(),
            "--id",
            "40",
            "--table",
            bin.to_str().unwrap(),
            "-o",
            bin.to_str().unwrap(),
        ]);
        assert!(res.is_err());
    }

    #[test]
    fn validate_runs_and_reports_json() {
        let (res, text) = run_args(&[
            "validate",
            "29",
            "--profile",
            "s",
            "--frames",
            "2",
            "--threads",
            "2",
            "--quiet",
            "--json",
        ]);
        let outcome = res.unwrap();
        assert!(text.contains("\"shape_stats\""), "{text}");
        assert_eq!(
            text.contains("\"passed\":true"),
            outcome == Outcome::Success,
            "{text}"
        );
    }
}
