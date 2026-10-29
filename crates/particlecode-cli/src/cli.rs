//! Command line definition (clap derive) and the small value parsers it uses.
//!
//! Adding a subcommand: add a variant to [`Command`], an argument struct here, a module
//! with its logic and one arm in [`crate::run`].

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use particlecode_core::motion::Mood;
use particlecode_core::profile::Profile;
use particlecode_core::shapes::ShapeTable;

/// `particlecode`: encode payloads into animated dot codes and inspect the format.
#[derive(Debug, Parser)]
#[command(name = "particlecode", version, about, long_about = None)]
pub struct Cli {
    /// What to do.
    #[command(subcommand)]
    pub command: Command,
}

/// The subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Render a payload as PNG, animated GIF, SVG or a PNG sequence.
    Encode(EncodeArgs),
    /// Print the capacity of every profile and shape.
    Info(InfoArgs),
    /// Symbol-level self-test: session, simulated losses and errors, receiver.
    Roundtrip(RoundtripArgs),
    /// Recover a payload from camera images (PNG files or directories of them).
    Decode(DecodeArgs),
}

/// Lattice profile argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProfileArg {
    /// 48 dots across.
    S,
    /// 72 dots across (default).
    M,
    /// 96 dots across.
    L,
}

impl From<ProfileArg> for Profile {
    fn from(p: ProfileArg) -> Profile {
        match p {
            ProfileArg::S => Profile::S,
            ProfileArg::M => Profile::M,
            ProfileArg::L => Profile::L,
        }
    }
}

/// Output format argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum FormatArg {
    /// First frame (or `--frame i`) as a PNG.
    Png,
    /// Looping animated GIF of one or more passes.
    Gif,
    /// Static vector frame.
    Svg,
    /// Directory of PNG frames plus `manifest.json`.
    Seq,
}

/// Animation mood argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum MoodArg {
    /// A slow wave travelling across the field.
    Breathing,
    /// A smooth noise field, like wind through grass.
    Flow,
}

impl From<MoodArg> for Mood {
    fn from(m: MoodArg) -> Mood {
        match m {
            MoodArg::Breathing => Mood::Breathing,
            MoodArg::Flow => Mood::FlowField,
        }
    }
}

/// Arguments of `encode`.
#[derive(Debug, Args)]
#[allow(clippy::struct_excessive_bools)] // independent switches, one flag each
pub struct EncodeArgs {
    /// Payload file, or `-` for standard input.
    pub input: PathBuf,
    /// Output file (`seq`: output directory).
    #[arg(short, long)]
    pub output: PathBuf,
    /// Output format; inferred from the extension of `-o` when omitted (no extension: seq).
    #[arg(short, long, value_enum)]
    pub format: Option<FormatArg>,
    /// Lattice profile.
    #[arg(short, long, value_enum, default_value = "m")]
    pub profile: ProfileArg,
    /// Required shape: disc, heart, A..Z, rounded-square, hexagon, ring or a numeric id.
    #[arg(long, value_parser = parse_shape, conflicts_with = "prefer_shape")]
    pub shape: Option<u8>,
    /// Preferred shape, falling back to the disc when it is unknown or too small. Repeat
    /// to list several in order of preference.
    #[arg(long, value_parser = parse_shape)]
    pub prefer_shape: Vec<u8>,
    /// Emit one self-contained frame with the stronger ECC (print, poster).
    #[arg(long = "static")]
    pub is_static: bool,
    /// Segment size is `1024 << N` bytes.
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u8).range(0..=15))]
    pub segment_kb_log2: u8,
    /// Image side in pixels (default: 800 png/svg, 640 seq, 480 gif).
    #[arg(long)]
    pub size: Option<u32>,
    /// Data frames per second (gif, seq and the liveness clock).
    #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u16).range(1..=240))]
    pub fps: u16,
    /// Animation mood for gif output.
    #[arg(long, value_enum, default_value = "breathing")]
    pub mood: MoodArg,
    /// Switch on every optional motion layer in gif output (ripple, anchor heartbeat,
    /// particle trails, reveal, shimmer).
    #[arg(long)]
    pub all_moods: bool,
    /// Passes over the whole payload in gif and seq output.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=64))]
    pub passes: u32,
    /// Frame index for png and svg output.
    #[arg(long, default_value_t = 0)]
    pub frame: u32,
    /// Session id as up to 4 hex digits (default: random).
    #[arg(long, value_parser = parse_session_id)]
    pub session_id: Option<u16>,
    /// Session key as 64 hex digits (default: random).
    #[arg(long, value_parser = parse_key)]
    pub key: Option<[u8; 32]>,
    /// Unix second at which frame 0 is shown, for the liveness particles (default: now).
    #[arg(long)]
    pub clock_origin: Option<u64>,
}

/// Arguments of `info`.
#[derive(Debug, Args)]
pub struct InfoArgs {
    /// Only these shapes (repeatable); default: every built-in shape.
    #[arg(long, value_parser = parse_shape)]
    pub shape: Vec<u8>,
    /// Only these profiles (repeatable); default: all.
    #[arg(short, long, value_enum)]
    pub profile: Vec<ProfileArg>,
}

/// Arguments of `roundtrip`.
#[derive(Debug, Args)]
pub struct RoundtripArgs {
    /// Payload file, or `-` for standard input.
    pub input: PathBuf,
    /// Lattice profile.
    #[arg(short, long, value_enum, default_value = "m")]
    pub profile: ProfileArg,
    /// Required shape (see `encode --shape`).
    #[arg(long, value_parser = parse_shape)]
    pub shape: Option<u8>,
    /// Use a static (high-ECC, single frame) session.
    #[arg(long = "static")]
    pub is_static: bool,
    /// Segment size is `1024 << N` bytes.
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u8).range(0..=15))]
    pub segment_kb_log2: u8,
    /// Drop every N-th frame (0: none).
    #[arg(long, default_value_t = 0)]
    pub drop_every: u32,
    /// Probability that a received symbol is replaced by a wrong one, `0.0..=1.0`
    /// (symbol-level only).
    #[arg(long, default_value_t = 0.0, value_parser = parse_rate, conflicts_with = "camera")]
    pub error_rate: f64,
    /// Seed of the simulated losses and errors.
    #[arg(long, default_value_t = 1)]
    pub seed: u64,
    /// Give up after this many frames (default: ten passes).
    #[arg(long)]
    pub max_frames: Option<u32>,
    /// Print one line per frame.
    #[arg(short, long)]
    pub verbose: bool,
    /// Image-level round trip: render each frame with breathing motion, degrade it like a
    /// phone photo and decode it with the vision stage.
    #[arg(long)]
    pub camera: bool,
    /// Camera blur sigma in pixels.
    #[arg(long, default_value_t = 0.9, requires = "camera", value_parser = parse_non_negative)]
    pub blur: f64,
    /// Camera noise sigma (0..255 units).
    #[arg(long, default_value_t = 4.0, requires = "camera", value_parser = parse_non_negative)]
    pub noise: f64,
    /// Camera tilt in degrees (`0..=80`).
    #[arg(long, default_value_t = 12.0, requires = "camera", value_parser = parse_tilt)]
    pub tilt: f64,
}

/// Arguments of `decode`.
#[derive(Debug, Args)]
pub struct DecodeArgs {
    /// PNG files, or directories (every `*.png` in them, sorted by name), in feed order.
    #[arg(required = true)]
    pub inputs: Vec<PathBuf>,
    /// Where to write the payload (default: standard output unless it is a terminal, else
    /// `decoded.bin`).
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// One JSON object per image instead of a text line.
    #[arg(long)]
    pub json: bool,
}

/// Resolve a shape argument: a built-in name (`disc`, `rounded-square`, `heart`, `hexagon`,
/// `ring`, `A`..`Z`, case-insensitive) or a numeric id.
pub fn parse_shape(s: &str) -> Result<u8, String> {
    let wanted = s.trim().to_ascii_lowercase().replace('_', "-");
    let wanted = if wanted == "square" {
        "rounded-square".to_owned()
    } else {
        wanted
    };
    let table = ShapeTable::builtin();
    if let Some(id) = table.ids().find(|&id| {
        table
            .name(id)
            .is_some_and(|n| n.to_ascii_lowercase() == wanted)
    }) {
        return Ok(id);
    }
    wanted.parse::<u8>().map_err(|_| {
        format!("unknown shape {s:?}; use disc, heart, rounded-square, hexagon, ring, A..Z or an id 0-255")
    })
}

/// Parse a session id given as up to four hex digits (optional `0x` prefix).
pub fn parse_session_id(s: &str) -> Result<u16, String> {
    let digits = s.trim().trim_start_matches("0x");
    if digits.is_empty() || digits.len() > 4 {
        return Err(format!("session id {s:?} must be 1-4 hex digits"));
    }
    u16::from_str_radix(digits, 16).map_err(|_| format!("session id {s:?} is not hexadecimal"))
}

/// Parse a 32-byte key given as 64 hex digits.
pub fn parse_key(s: &str) -> Result<[u8; 32], String> {
    let digits = s.trim().trim_start_matches("0x");
    if digits.len() != 64 || !digits.is_ascii() {
        return Err("key must be exactly 64 hex digits".to_owned());
    }
    let mut key = [0u8; 32];
    for (byte, pair) in key.iter_mut().zip(digits.as_bytes().chunks(2)) {
        let pair = std::str::from_utf8(pair).map_err(|_| "key is not hexadecimal".to_owned())?;
        *byte = u8::from_str_radix(pair, 16).map_err(|_| "key is not hexadecimal".to_owned())?;
    }
    Ok(key)
}

/// Lowercase hex of `bytes`.
pub fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Parse a probability in `0.0..=1.0`.
pub fn parse_rate(s: &str) -> Result<f64, String> {
    let v: f64 = s
        .trim()
        .parse()
        .map_err(|_| format!("{s:?} is not a number"))?;
    if (0.0..=1.0).contains(&v) {
        Ok(v)
    } else {
        Err(format!("{s:?} is outside 0.0..=1.0"))
    }
}

/// Parse a finite number `>= 0`.
pub fn parse_non_negative(s: &str) -> Result<f64, String> {
    let v: f64 = s
        .trim()
        .parse()
        .map_err(|_| format!("{s:?} is not a number"))?;
    if v.is_finite() && (0.0..=1000.0).contains(&v) {
        Ok(v)
    } else {
        Err(format!("{s:?} must be between 0 and 1000"))
    }
}

/// Parse a tilt angle in `0.0..=80.0` degrees.
pub fn parse_tilt(s: &str) -> Result<f64, String> {
    let v: f64 = s
        .trim()
        .parse()
        .map_err(|_| format!("{s:?} is not a number"))?;
    if (0.0..=80.0).contains(&v) {
        Ok(v)
    } else {
        Err(format!("{s:?} is outside 0..=80 degrees"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("particlecode").chain(args.iter().copied()))
    }

    #[test]
    fn shapes_resolve_by_name_and_id() {
        assert_eq!(parse_shape("disc"), Ok(0));
        assert_eq!(parse_shape("Rounded-Square"), Ok(1));
        assert_eq!(parse_shape("rounded_square"), Ok(1));
        assert_eq!(parse_shape("square"), Ok(1));
        assert_eq!(parse_shape("A"), Ok(2));
        assert_eq!(parse_shape("z"), Ok(27));
        assert_eq!(parse_shape("heart"), Ok(28));
        assert_eq!(parse_shape("hexagon"), Ok(29));
        assert_eq!(parse_shape("ring"), Ok(30));
        assert_eq!(parse_shape("77"), Ok(77));
        assert!(parse_shape("banana").is_err());
        assert!(parse_shape("300").is_err());
        assert!(parse_shape("").is_err());
    }

    #[test]
    fn session_ids_and_keys() {
        assert_eq!(parse_session_id("beef"), Ok(0xBEEF));
        assert_eq!(parse_session_id("0x1F"), Ok(0x1F));
        assert!(parse_session_id("").is_err());
        assert!(parse_session_id("12345").is_err());
        assert!(parse_session_id("xyz").is_err());
        let hex = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let key = parse_key(hex).expect("valid key");
        assert_eq!(key[1], 0x11);
        assert_eq!(to_hex(&key), hex);
        assert!(parse_key("00").is_err());
        assert!(parse_key(&"zz".repeat(32)).is_err());
        assert!(parse_key(&"é".repeat(32)).is_err());
    }

    #[test]
    fn rates() {
        assert_eq!(parse_rate("0.25"), Ok(0.25));
        assert!(parse_rate("1.5").is_err());
        assert!(parse_rate("-0.1").is_err());
        assert!(parse_rate("nan").is_err());
        assert!(parse_rate("x").is_err());
    }

    #[test]
    fn encode_defaults() {
        let cli = parse(&["encode", "in.cbor", "-o", "out.png"]).expect("parses");
        let Command::Encode(a) = cli.command else {
            panic!("not encode");
        };
        assert_eq!(a.profile, ProfileArg::M);
        assert_eq!(a.format, None);
        assert_eq!(a.shape, None);
        assert!(a.prefer_shape.is_empty());
        assert!(!a.is_static);
        assert_eq!((a.segment_kb_log2, a.fps, a.passes, a.frame), (3, 10, 1, 0));
    }

    #[test]
    fn encode_full_command_line() {
        let key = "ab".repeat(32);
        let cli = parse(&[
            "encode",
            "-",
            "-o",
            "out",
            "--format",
            "gif",
            "--profile",
            "l",
            "--prefer-shape",
            "heart",
            "--prefer-shape",
            "P",
            "--static",
            "--segment-kb-log2",
            "4",
            "--size",
            "320",
            "--fps",
            "12",
            "--mood",
            "flow",
            "--all-moods",
            "--passes",
            "2",
            "--session-id",
            "1a2b",
            "--key",
            &key,
        ])
        .expect("parses");
        let Command::Encode(a) = cli.command else {
            panic!("not encode");
        };
        assert_eq!(a.format, Some(FormatArg::Gif));
        assert_eq!(a.profile, ProfileArg::L);
        assert_eq!(a.prefer_shape, vec![28, 17]);
        assert!(a.is_static && a.all_moods);
        assert_eq!(a.mood, MoodArg::Flow);
        assert_eq!(a.size, Some(320));
        assert_eq!(a.session_id, Some(0x1A2B));
        assert_eq!(a.key, Some([0xAB; 32]));
        assert_eq!((a.segment_kb_log2, a.fps, a.passes), (4, 12, 2));
    }

    #[test]
    fn bad_arguments_are_rejected() {
        assert!(parse(&["encode", "in", "-o", "x", "--profile", "xl"]).is_err());
        assert!(parse(&["encode", "in", "-o", "x", "--segment-kb-log2", "16"]).is_err());
        assert!(
            parse(&[
                "encode",
                "in",
                "-o",
                "x",
                "--shape",
                "heart",
                "--prefer-shape",
                "A"
            ])
            .is_err()
        );
        assert!(parse(&["encode", "in"]).is_err());
        assert!(parse(&["encode", "in", "-o", "x", "--fps", "0"]).is_err());
        assert!(parse(&["roundtrip", "in", "--error-rate", "2"]).is_err());
        assert!(parse(&["nonsense"]).is_err());
        assert!(parse(&[]).is_err());
    }

    #[test]
    fn decode_and_camera_flags() {
        let cli = parse(&["decode", "a.png", "dir", "-o", "out.bin", "--json"]).expect("parses");
        let Command::Decode(d) = cli.command else {
            panic!("not decode");
        };
        assert_eq!(d.inputs.len(), 2);
        assert_eq!(d.output.as_deref(), Some(std::path::Path::new("out.bin")));
        assert!(d.json);
        assert!(parse(&["decode"]).is_err());
        let cli = parse(&[
            "roundtrip",
            "f",
            "--camera",
            "--blur",
            "1.5",
            "--tilt",
            "20",
        ])
        .expect("parses");
        let Command::Roundtrip(r) = cli.command else {
            panic!("not roundtrip");
        };
        assert!(r.camera);
        assert!((r.blur - 1.5).abs() < f64::EPSILON && (r.noise - 4.0).abs() < f64::EPSILON);
        assert!(
            parse(&["roundtrip", "f", "--blur", "1"]).is_err(),
            "needs --camera"
        );
        assert!(parse(&["roundtrip", "f", "--camera", "--error-rate", "0.1"]).is_err());
        assert!(parse(&["roundtrip", "f", "--camera", "--tilt", "90"]).is_err());
        assert!(parse(&["roundtrip", "f", "--camera", "--noise", "-1"]).is_err());
    }

    #[test]
    fn info_and_roundtrip_parse() {
        let cli = parse(&["info", "--shape", "disc", "--shape", "A", "-p", "s"]).expect("parses");
        let Command::Info(i) = cli.command else {
            panic!("not info");
        };
        assert_eq!(i.shape, vec![0, 2]);
        assert_eq!(i.profile, vec![ProfileArg::S]);
        let cli = parse(&[
            "roundtrip",
            "f",
            "--drop-every",
            "4",
            "--error-rate",
            "0.02",
            "-v",
        ])
        .expect("parses");
        let Command::Roundtrip(r) = cli.command else {
            panic!("not roundtrip");
        };
        assert_eq!(r.drop_every, 4);
        assert!((r.error_rate - 0.02).abs() < f64::EPSILON);
        assert!(r.verbose);
    }
}
