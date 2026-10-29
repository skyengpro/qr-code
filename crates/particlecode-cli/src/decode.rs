//! `decode`: camera images (PNG files) to the payload.
//!
//! [`Decoder`] is the image-level receiver shared by the `decode` subcommand, the camera
//! round trip ([`crate::camera`]) and the examples: `locate`, `read`, then
//! [`Receiver::feed_decoded`].

use std::fs;
use std::io::{Cursor, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use particlecode_core::shapes::ShapeTable;
use particlecode_core::{DecodeError, Header, Liveness, Profile, Progress, Receiver};
use particlecode_vision::{VisionError, YuvFrame, locate, read};
use serde_json::{Value, json};

use crate::cli::DecodeArgs;

/// What happened to one image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// No Particle Code anchors were found.
    NoLock,
    /// Anchors were found but the frame could not be read (cut off, bad calibration, ...).
    Unreadable(VisionError),
    /// The receiver refused the frame (for example too many symbol errors, or another
    /// session).
    Rejected(DecodeError),
    /// The receiver used the frame.
    Accepted {
        /// Distinct symbols held, counting at most `K_s` per segment.
        have: u32,
        /// Symbols needed if nothing is lost.
        need: u32,
        /// Segments verified so far.
        segments_done: u32,
        /// Segments in the transfer.
        segments_total: u32,
    },
    /// The payload is complete (see [`Decoder::payload`]).
    Done,
}

/// Everything known about one fed image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameReport {
    /// What the receiver did with it.
    pub outcome: Outcome,
    /// The decoded frame header, when the image got that far.
    pub header: Option<Header>,
    /// What the particles showed.
    pub liveness: Liveness,
}

impl FrameReport {
    /// The anchors were found (every outcome except [`Outcome::NoLock`]).
    pub fn locked(&self) -> bool {
        self.outcome != Outcome::NoLock
    }
}

/// Image-level receiver: feed camera images, collect the payload.
#[derive(Debug, Default)]
pub struct Decoder {
    receiver: Receiver,
    payload: Option<Vec<u8>>,
    names: ShapeTable,
}

impl Decoder {
    /// A decoder with the built-in shape table.
    pub fn new() -> Decoder {
        Decoder::default()
    }

    /// The finished payload, once an image completed the transfer.
    pub fn payload(&self) -> Option<&[u8]> {
        self.payload.as_deref()
    }

    /// Display name of a shape id.
    pub fn shape_name(&self, id: u8) -> String {
        self.names
            .name(id)
            .map_or_else(|| format!("#{id}"), str::to_owned)
    }

    /// Locate, read and feed one camera frame.
    pub fn feed_yuv(&mut self, yuv: &YuvFrame) -> FrameReport {
        let Some(lock) = locate(&yuv.y) else {
            return FrameReport {
                outcome: Outcome::NoLock,
                header: None,
                liveness: Liveness::Unknown,
            };
        };
        let receiver = &self.receiver;
        let read = read(yuv, &lock, &|p: Profile, s: u8| receiver.dot_order(p, s));
        let (header, body, liveness) = match read {
            Ok(r) => r,
            Err(e) => {
                return FrameReport {
                    outcome: Outcome::Unreadable(e),
                    header: None,
                    liveness: Liveness::Unknown,
                };
            }
        };
        let outcome = match self.receiver.feed_decoded(&header, &body) {
            Progress::Rejected(e) => Outcome::Rejected(e),
            Progress::Accepted {
                have,
                need,
                segments_done,
                segments_total,
            } => Outcome::Accepted {
                have,
                need,
                segments_done,
                segments_total,
            },
            Progress::Done(data) => {
                self.payload = Some(data);
                Outcome::Done
            }
        };
        FrameReport {
            outcome,
            header: Some(header),
            liveness,
        }
    }

    /// Progress so far, from the most recent accepted frame's counters (`None` before the
    /// first accepted frame).
    pub fn progress_of(report: &FrameReport) -> Option<(u32, u32, u32, u32)> {
        match report.outcome {
            Outcome::Accepted {
                have,
                need,
                segments_done,
                segments_total,
            } => Some((have, need, segments_done, segments_total)),
            _ => None,
        }
    }
}

/// Decode a PNG (8-bit gray, gray+alpha, RGB or RGBA, palettes expanded, 16-bit reduced)
/// into a YUV frame. Alpha is ignored.
pub fn yuv_from_png(bytes: &[u8]) -> Result<YuvFrame> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().context("not a valid PNG")?;
    let size = reader
        .output_buffer_size()
        .context("image dimensions overflow")?;
    let mut buf = vec![0u8; size];
    let info = reader.next_frame(&mut buf).context("cannot decode PNG")?;
    if info.bit_depth != png::BitDepth::Eight {
        bail!("unsupported PNG bit depth {:?}", info.bit_depth);
    }
    let (w, h) = (info.width as usize, info.height as usize);
    let data = &buf[..info.buffer_size()];
    let rgb: Vec<u8> = match info.color_type {
        png::ColorType::Rgb => data.to_vec(),
        png::ColorType::Rgba => data
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g]).collect(),
        png::ColorType::GrayscaleAlpha => data
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0]])
            .collect(),
        png::ColorType::Indexed => bail!("palette PNG was not expanded"),
    };
    let yuv = YuvFrame::from_rgb(w, h, &rgb);
    if yuv.y.width == 0 {
        bail!("empty image");
    }
    Ok(yuv)
}

/// Expand the inputs: files stay, directories become their `*.png` files sorted by name.
pub fn collect_inputs(inputs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for input in inputs {
        if input.is_dir() {
            let mut pngs: Vec<PathBuf> = fs::read_dir(input)
                .with_context(|| format!("cannot read directory {}", input.display()))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.is_file()
                        && p.extension()
                            .and_then(|e| e.to_str())
                            .is_some_and(|e| e.eq_ignore_ascii_case("png"))
                })
                .collect();
            pngs.sort();
            if pngs.is_empty() {
                bail!("no .png files in {}", input.display());
            }
            out.extend(pngs);
        } else {
            out.push(input.clone());
        }
    }
    Ok(out)
}

/// One-line description of an image's result.
pub fn describe(name: &str, report: &FrameReport, decoder: &Decoder) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(match &report.outcome {
        Outcome::NoLock => "no lock".to_owned(),
        Outcome::Unreadable(e) => format!("locked, unreadable ({e})"),
        _ => "locked".to_owned(),
    });
    if let Some(h) = &report.header {
        parts.push(format!(
            "profile {:?}, shape {}, segment {}, seed {}",
            h.profile,
            decoder.shape_name(h.shape_id),
            h.segment_index,
            h.packet_seed
        ));
    }
    match &report.outcome {
        Outcome::Rejected(e) => parts.push(format!("rejected ({e})")),
        Outcome::Accepted {
            have,
            need,
            segments_done,
            segments_total,
        } => parts.push(format!(
            "have {have}/{need}, segments {segments_done}/{segments_total}"
        )),
        Outcome::Done => parts.push("DONE".to_owned()),
        _ => {}
    }
    if report.header.is_some() {
        parts.push(match report.liveness {
            Liveness::Unknown => "liveness unknown".to_owned(),
            Liveness::Read {
                seed_low,
                rolling,
                consistent,
            } => format!(
                "liveness seed_low {seed_low:#04x} rolling {rolling:#06x} {}",
                if consistent {
                    "consistent"
                } else {
                    "INCONSISTENT"
                }
            ),
        });
    }
    format!("{name}: {}", parts.join(", "))
}

/// JSON object (one per image) for `--json`.
pub fn to_json(name: &str, report: &FrameReport, decoder: &Decoder) -> Value {
    let liveness = match report.liveness {
        Liveness::Unknown => json!({ "state": "unknown" }),
        Liveness::Read {
            seed_low,
            rolling,
            consistent,
        } => json!({
            "state": "read",
            "seed_low": seed_low,
            "rolling": rolling,
            "consistent": consistent,
        }),
    };
    let (status, detail) = match &report.outcome {
        Outcome::NoLock => ("no_lock", Value::Null),
        Outcome::Unreadable(e) => ("unreadable", json!(e.to_string())),
        Outcome::Rejected(e) => ("rejected", json!(e.to_string())),
        Outcome::Accepted { .. } => ("accepted", Value::Null),
        Outcome::Done => ("done", Value::Null),
    };
    let progress = Decoder::progress_of(report).map(|(have, need, done, total)| {
        json!({ "have": have, "need": need, "segments_done": done, "segments_total": total })
    });
    json!({
        "file": name,
        "locked": report.locked(),
        "status": status,
        "detail": detail,
        "profile": report.header.as_ref().map(|h| format!("{:?}", h.profile)),
        "shape": report.header.as_ref().map(|h| decoder.shape_name(h.shape_id)),
        "segment": report.header.as_ref().map(|h| h.segment_index),
        "seed": report.header.as_ref().map(|h| h.packet_seed),
        "progress": progress,
        "liveness": liveness,
    })
}

/// Execute `decode`: feed the images in order until the payload is complete.
///
/// The payload goes to `-o`, or to standard output when that is not a terminal, else to
/// `decoded.bin`. Per-image lines go to `out` (to `err` when the payload itself uses
/// standard output). Fails with the have/need counters if the images are not enough.
pub fn command(
    args: &DecodeArgs,
    stdout_is_terminal: bool,
    out: &mut impl Write,
    err: &mut impl Write,
) -> Result<()> {
    let files = collect_inputs(&args.inputs)?;
    let payload_to_stdout = args.output.is_none() && !stdout_is_terminal;
    let mut decoder = Decoder::new();
    let (mut images, mut locked, mut rejected, mut bad) = (0u32, 0u32, 0u32, 0u32);
    let mut last_progress = None;

    for path in &files {
        let name = path.display().to_string();
        images += 1;
        let report = match fs::read(path)
            .with_context(|| format!("cannot read {name}"))
            .and_then(|bytes| yuv_from_png(&bytes))
        {
            Ok(yuv) => decoder.feed_yuv(&yuv),
            Err(e) => {
                bad += 1;
                let line = if args.json {
                    json!({ "file": name, "locked": false, "status": "bad_image",
                            "detail": format!("{e:#}") })
                    .to_string()
                } else {
                    format!("{name}: skipped ({e:#})")
                };
                let sink: &mut dyn Write = if payload_to_stdout { err } else { out };
                writeln!(sink, "{line}")?;
                continue;
            }
        };
        locked += u32::from(report.locked());
        rejected += u32::from(matches!(report.outcome, Outcome::Rejected(_)));
        if let Some(p) = Decoder::progress_of(&report) {
            last_progress = Some(p);
        }
        let line = if args.json {
            to_json(&name, &report, &decoder).to_string()
        } else {
            describe(&name, &report, &decoder)
        };
        let sink: &mut dyn Write = if payload_to_stdout { err } else { out };
        writeln!(sink, "{line}")?;
        if report.outcome == Outcome::Done {
            break;
        }
    }

    let Some(payload) = decoder.payload() else {
        let progress = match last_progress {
            Some((have, need, done, total)) => {
                format!("have {have}/{need} symbols, {done}/{total} segments")
            }
            None => "no frame was accepted".to_owned(),
        };
        bail!(
            "transfer incomplete: {progress} ({images} image(s), {locked} locked, {rejected} \
             rejected, {bad} unreadable)"
        );
    };
    if payload_to_stdout {
        out.write_all(payload)?;
        out.flush()?;
        writeln!(err, "decoded {} bytes to standard output", payload.len())?;
    } else {
        let path = args
            .output
            .clone()
            .unwrap_or_else(|| Path::new("decoded.bin").to_path_buf());
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            fs::create_dir_all(dir)?;
        }
        fs::write(&path, payload).with_context(|| format!("cannot write {}", path.display()))?;
        writeln!(out, "decoded {} bytes to {}", payload.len(), path.display())?;
    }
    Ok(())
}

/// Whether the real standard output is a terminal.
pub fn stdout_is_terminal() -> bool {
    std::io::stdout().is_terminal()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(width: u32, height: u32, color: png::ColorType, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(color);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("header");
        w.write_image_data(data).expect("data");
        w.finish().expect("finish");
        out
    }

    #[test]
    fn png_color_types_convert() {
        let rgb =
            yuv_from_png(&encode(2, 1, png::ColorType::Rgb, &[255, 0, 0, 0, 0, 255])).expect("rgb");
        assert_eq!((rgb.y.width, rgb.y.height), (2, 1));
        assert!(rgb.y.data[0] > 60 && rgb.y.data[0] < 90, "red luma");
        let rgba =
            yuv_from_png(&encode(1, 1, png::ColorType::Rgba, &[10, 200, 30, 0])).expect("rgba");
        assert_eq!(rgba.y.width, 1);
        let gray = yuv_from_png(&encode(2, 2, png::ColorType::Grayscale, &[0, 64, 128, 255]))
            .expect("gray");
        assert_eq!(gray.y.data, vec![0, 64, 128, 255]);
        let ga = yuv_from_png(&encode(1, 1, png::ColorType::GrayscaleAlpha, &[77, 5])).expect("ga");
        assert_eq!(ga.y.data, vec![77]);
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(yuv_from_png(b"not a png").is_err());
        assert!(yuv_from_png(&[]).is_err());
        let mut truncated = encode(8, 8, png::ColorType::Rgb, &[7; 192]);
        truncated.truncate(30);
        assert!(yuv_from_png(&truncated).is_err());
    }

    #[test]
    fn blank_images_do_not_lock() {
        let yuv =
            yuv_from_png(&encode(64, 64, png::ColorType::Grayscale, &[200; 4096])).expect("png");
        let mut d = Decoder::new();
        let r = d.feed_yuv(&yuv);
        assert_eq!(r.outcome, Outcome::NoLock);
        assert!(!r.locked());
        assert!(d.payload().is_none());
        assert!(describe("x.png", &r, &d).contains("no lock"));
        assert_eq!(to_json("x.png", &r, &d)["status"], "no_lock");
    }

    #[test]
    fn directories_expand_sorted() {
        let dir = std::env::temp_dir().join(format!("particlecode-decode-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir");
        for n in ["b.png", "a.PNG", "c.txt"] {
            fs::write(dir.join(n), b"x").expect("file");
        }
        let files = collect_inputs(std::slice::from_ref(&dir)).expect("collect");
        let names: Vec<_> = files
            .iter()
            .map(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .expect("name")
                    .to_owned()
            })
            .collect();
        assert_eq!(names, ["a.PNG", "b.png"]);
        let empty = dir.join("empty");
        fs::create_dir_all(&empty).expect("dir");
        assert!(collect_inputs(&[empty]).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn incomplete_transfer_is_an_error() {
        let dir = std::env::temp_dir().join(format!("particlecode-decode2-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir");
        fs::write(
            dir.join("blank.png"),
            encode(32, 32, png::ColorType::Grayscale, &[90; 1024]),
        )
        .expect("png");
        fs::write(dir.join("broken.png"), b"junk").expect("junk");
        let args = DecodeArgs {
            inputs: vec![dir.clone()],
            output: None,
            json: true,
        };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let e = command(&args, false, &mut out, &mut err).expect_err("nothing decodes");
        assert!(e.to_string().contains("transfer incomplete"), "{e}");
        // payload would go to stdout, so the per-image lines went to stderr as JSON.
        let text = String::from_utf8(err).expect("utf8");
        assert!(
            text.contains("bad_image") && text.contains("no_lock"),
            "{text}"
        );
        assert!(out.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
