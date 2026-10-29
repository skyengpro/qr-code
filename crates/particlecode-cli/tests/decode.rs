//! End-to-end tests of `decode` and `roundtrip --camera`: encode with the binary, read the
//! pictures back with the binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use particlecode_cli::sample;

/// A fresh, unique directory under the system temp dir, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let dir =
            std::env::temp_dir().join(format!("particlecode-dec-{}-{name}", std::process::id()));
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

fn s(p: &Path) -> &str {
    p.to_str().expect("utf-8 path")
}

/// Write `bytes` as `payload.cbor` into `dir` and encode it as a PNG sequence.
fn encode_sequence(dir: &TempDir, bytes: &[u8]) -> (PathBuf, PathBuf) {
    let input = dir.path().join("payload.cbor");
    fs::write(&input, bytes).expect("write payload");
    let frames = dir.path().join("frames");
    run_ok(&["encode", s(&input), "-o", s(&frames), "--size", "760"]);
    (input, frames)
}

#[test]
fn decode_recovers_a_sequence_byte_for_byte() {
    let dir = TempDir::new("seq");
    let data = sample::credential(3000);
    let (_, frames) = encode_sequence(&dir, &data);
    assert!(frames.join("manifest.json").is_file());

    // To a file, with a per-image line for every frame that was needed.
    let out = dir.path().join("decoded.bin");
    let res = run_ok(&["decode", s(&frames), "-o", s(&out)]);
    assert_eq!(fs::read(&out).expect("decoded file"), data);
    let text = String::from_utf8_lossy(&res.stdout);
    assert!(text.contains("locked, profile M, shape disc"), "{text}");
    assert!(text.contains("liveness") && text.contains("DONE"), "{text}");

    // To standard output (not a terminal under test), with the log on stderr.
    let res = run_ok(&["decode", s(&frames)]);
    assert_eq!(res.stdout, data);
    assert!(String::from_utf8_lossy(&res.stderr).contains("DONE"));

    // JSON lines.
    let res = run_ok(&["decode", s(&frames), "-o", s(&out), "--json"]);
    let last = String::from_utf8_lossy(&res.stdout)
        .lines()
        .rev()
        .find(|l| l.starts_with('{'))
        .expect("a JSON line")
        .to_owned();
    let v: serde_json::Value = serde_json::from_str(&last).expect("JSON");
    assert_eq!(v["status"], "done");
    assert_eq!(v["liveness"]["state"], "read");
}

#[test]
fn decode_reports_an_incomplete_transfer() {
    let dir = TempDir::new("short");
    let (_, frames) = encode_sequence(&dir, &sample::credential(6000));
    let first = frames.join("frame_00000.png");
    let res = run(&["decode", s(&first), s(&dir.path().join("nope.png"))]);
    assert_eq!(res.status.code(), Some(1));
    let err = String::from_utf8_lossy(&res.stderr);
    assert!(err.contains("transfer incomplete: have 1/5"), "{err}");
    assert!(err.contains("skipped"), "{err}");
    assert!(!err.contains("panicked"));

    // Garbage and empty directories never panic.
    let junk = dir.path().join("junk.png");
    fs::write(&junk, b"\x89PNG junk").expect("junk");
    let res = run(&["decode", s(&junk)]);
    assert_eq!(res.status.code(), Some(1));
    assert!(!String::from_utf8_lossy(&res.stderr).contains("panicked"));
    let empty = dir.path().join("empty");
    fs::create_dir_all(&empty).expect("dir");
    let res = run(&["decode", s(&empty)]);
    assert_eq!(res.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&res.stderr).contains("no .png files"));
}

#[test]
fn roundtrip_through_the_synthetic_camera() {
    let dir = TempDir::new("camera");
    let input = dir.path().join("payload.cbor");
    fs::write(&input, sample::credential(2000)).expect("payload");
    let res = run_ok(&[
        "roundtrip",
        s(&input),
        "--camera",
        "--profile",
        "s",
        "--drop-every",
        "4",
        "--tilt",
        "10",
    ]);
    let text = String::from_utf8_lossy(&res.stdout);
    assert!(text.contains("camera: blur 0.9"), "{text}");
    assert!(text.contains("0 lock failures"), "{text}");
    assert!(text.contains("OK:"), "{text}");
}
