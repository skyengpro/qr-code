//! End-to-end tests of the `particlecode` binary: run it, check what lands on disk.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use particlecode_cli::sample;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// A fresh, unique directory under the system temp dir, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let dir =
            std::env::temp_dir().join(format!("particlecode-cli-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_particlecode"))
        .args(args)
        .output()
        .expect("run particlecode")
}

fn run_ok(args: &[&str]) -> Output {
    let out = run(args);
    assert!(
        out.status.success(),
        "{args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

/// Write the sample payload into `dir` and return its path.
fn payload(dir: &TempDir, bytes: &[u8]) -> PathBuf {
    let path = dir.path().join("payload.cbor");
    fs::write(&path, bytes).expect("write payload");
    path
}

fn s(p: &Path) -> &str {
    p.to_str().expect("utf-8 path")
}

const IDENTITY: [&str; 4] = ["--session-id", "c0de", "--clock-origin", "1780000000"];

#[test]
fn encode_png() {
    let dir = TempDir::new("png");
    let input = payload(&dir, &sample::ticket());
    let out = dir.path().join("sub/frame.png");
    let res = run_ok(
        &[
            &["encode", s(&input), "-o", s(&out), "--size", "400"],
            &IDENTITY[..],
        ]
        .concat(),
    );
    let summary = String::from_utf8_lossy(&res.stderr);
    assert!(summary.contains("profile M"), "{summary}");
    assert!(summary.contains("session id c0de"), "{summary}");
    assert!(summary.contains("key "), "{summary}");

    let bytes = fs::read(&out).expect("png written");
    assert_eq!(bytes[..8], PNG_SIGNATURE);
    let decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
    let reader = decoder.read_info().expect("valid png");
    assert_eq!((reader.info().width, reader.info().height), (400, 400));
}

#[test]
fn encode_png_is_deterministic_with_identity() {
    let dir = TempDir::new("det");
    let input = payload(&dir, &sample::ticket());
    let (a, b) = (dir.path().join("a.png"), dir.path().join("b.png"));
    let key = "11".repeat(32);
    for out in [&a, &b] {
        run_ok(
            &[
                &[
                    "encode",
                    s(&input),
                    "-o",
                    s(out),
                    "--size",
                    "300",
                    "--key",
                    &key,
                ],
                &IDENTITY[..],
            ]
            .concat(),
        );
    }
    assert_eq!(fs::read(&a).expect("a"), fs::read(&b).expect("b"));
}

#[test]
fn encode_svg_from_stdin_with_a_letter_shape() {
    let dir = TempDir::new("svg");
    let out = dir.path().join("p.svg");
    let mut child = Command::new(env!("CARGO_BIN_EXE_particlecode"))
        .args([
            "encode",
            "-",
            "-o",
            s(&out),
            "--shape",
            "P",
            "--profile",
            "l",
        ])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(&sample::ticket())
        .expect("feed stdin");
    let res = child.wait_with_output().expect("wait");
    assert!(
        res.status.success(),
        "{}",
        String::from_utf8_lossy(&res.stderr)
    );
    assert!(String::from_utf8_lossy(&res.stderr).contains("shape P"));
    let svg = fs::read_to_string(&out).expect("svg written");
    assert!(
        svg.starts_with("<svg") || svg.contains("<svg"),
        "{}",
        &svg[..80.min(svg.len())]
    );
    assert!(svg.contains("<circle"));
}

#[test]
fn encode_static_poster() {
    let dir = TempDir::new("static");
    let input = payload(&dir, &sample::ticket());
    let out = dir.path().join("poster.png");
    let res = run_ok(&[
        "encode",
        s(&input),
        "-o",
        s(&out),
        "--static",
        "--profile",
        "s",
    ]);
    assert!(String::from_utf8_lossy(&res.stderr).contains("1 frame(s) per pass"));
    assert_eq!(fs::read(&out).expect("png")[..8], PNG_SIGNATURE);
}

#[test]
fn encode_gif_has_one_gif_frame_per_data_frame() {
    let dir = TempDir::new("gif");
    let input = payload(&dir, &sample::credential(6000));
    let out = dir.path().join("loop.gif");
    run_ok(&[
        "encode",
        s(&input),
        "-o",
        s(&out),
        "--size",
        "200",
        "--passes",
        "2",
        "--mood",
        "flow",
    ]);

    let file = fs::File::open(&out).expect("gif written");
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::Indexed);
    let mut decoder = opts.read_info(file).expect("valid gif");
    assert_eq!((decoder.width(), decoder.height()), (200, 200));
    let mut count = 0;
    while decoder.read_next_frame().expect("frame").is_some() {
        count += 1;
    }
    // 6 KB at profile M: 5 source frames + 2 repair frames per pass, two passes.
    assert_eq!(count, 14);
}

#[test]
fn encode_sequence_writes_frames_and_manifest() {
    let dir = TempDir::new("seq");
    let input = payload(&dir, &sample::credential(3000));
    let out = dir.path().join("frames");
    run_ok(&[
        "encode",
        s(&input),
        "-o",
        s(&out),
        "--size",
        "240",
        "--profile",
        "s",
    ]);

    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(out.join("manifest.json")).expect("manifest"))
            .expect("manifest is JSON");
    let frames = manifest["frames"].as_array().expect("frames array");
    assert!(frames.len() >= 3);
    assert_eq!(manifest["frame_size_px"], 240);
    assert_eq!(manifest["profile"], "S");
    assert_eq!(
        manifest["payload_len"],
        3000_u64.max(sample::credential(3000).len() as u64)
    );
    for f in frames {
        let name = f["file"].as_str().expect("file name");
        let bytes = fs::read(out.join(name)).expect("frame file");
        assert_eq!(bytes[..8], PNG_SIGNATURE, "{name}");
    }
}

#[test]
fn info_prints_a_table() {
    let res = run_ok(&["info", "--shape", "disc", "--shape", "heart", "-p", "m"]);
    let text = String::from_utf8_lossy(&res.stdout);
    assert!(text.contains("disc") && text.contains("heart"), "{text}");
    assert_eq!(text.lines().filter(|l| l.starts_with("M ")).count(), 2);
}

#[test]
fn roundtrip_succeeds_and_reports() {
    let dir = TempDir::new("rt");
    let input = payload(&dir, &sample::credential(6000));
    let res = run_ok(&[
        "roundtrip",
        s(&input),
        "--drop-every",
        "3",
        "--error-rate",
        "0.01",
    ]);
    let text = String::from_utf8_lossy(&res.stdout);
    assert!(text.contains("OK:"), "{text}");
}

#[test]
fn bad_input_fails_cleanly() {
    let dir = TempDir::new("bad");
    let out = dir.path().join("x.png");

    let res = run(&["encode", s(&dir.path().join("missing.cbor")), "-o", s(&out)]);
    assert_eq!(res.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&res.stderr).starts_with("error: cannot read"));

    let input = payload(&dir, b"hi");
    let res = run(&["encode", s(&input), "-o", s(&out), "--shape", "200"]);
    assert_eq!(res.status.code(), Some(1));
    assert!(!String::from_utf8_lossy(&res.stderr).contains("panicked"));

    let res = run(&["encode", s(&input), "-o", s(&dir.path().join("x.jpg"))]);
    assert_eq!(res.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&res.stderr).contains("--format"));

    // Static payload that cannot fit one frame.
    let big = payload(&dir, &vec![9u8; 20_000]);
    let res = run(&[
        "encode",
        s(&big),
        "-o",
        s(&out),
        "--static",
        "--profile",
        "s",
    ]);
    assert_eq!(res.status.code(), Some(1));

    // clap rejects bad flags with exit code 2.
    let res = run(&["encode", s(&input), "-o", s(&out), "--profile", "xl"]);
    assert_eq!(res.status.code(), Some(2));

    // A hopeless channel exits non-zero without panicking.
    let res = run(&[
        "roundtrip",
        s(&input),
        "--error-rate",
        "0.7",
        "--max-frames",
        "5",
    ]);
    assert_eq!(res.status.code(), Some(1));
    assert!(!String::from_utf8_lossy(&res.stderr).contains("panicked"));
}

#[test]
fn empty_payload_does_not_panic() {
    let dir = TempDir::new("empty");
    let input = payload(&dir, b"");
    let out = dir.path().join("e.png");
    let res = run(&["encode", s(&input), "-o", s(&out)]);
    assert!(!String::from_utf8_lossy(&res.stderr).contains("panicked"));
}
