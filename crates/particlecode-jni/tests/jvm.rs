//! The JNI exports, exercised by a real JVM.
//!
//! The test renders a transfer with `particlecode-core` and `particlecode-render`,
//! photographs it with the synthetic camera, lays each frame out as Android delivers NV21
//! (padded rows, luma plus one interleaved `VU` buffer) and writes the bytes to a temp
//! directory. A Java twin of the Kotlin `NativeBridge` (same fully qualified name, same
//! `static native` signatures, so the same `Java_com_vaam_particlecode_NativeBridge_*`
//! symbols) loads the `libparticlecode_jni` cdylib built from this crate. The Java driver wraps the
//! bytes in direct `ByteBuffer`s (U and V as overlapping views of the `VU` buffer, pixel
//! stride 2), runs the transfer to `Done`, checks the payload, and probes the error paths
//! (null and freed handles, short and heap buffers, bad strides and arrays), which must
//! return codes and never crash the JVM.
//!
//! Without a JDK on `PATH` the test prints a note and returns.

#![allow(clippy::print_stdout, clippy::print_stderr)] // a test that reports what it ran

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use particlecode_core::motion::{MotionConfig, seed_from_session};
use particlecode_core::profile::Rgb;
use particlecode_core::{DisplayInfo, Session, SessionConfig};
use particlecode_jni::layout::*;
use particlecode_render::synth::{CameraPose, Degradation, RgbImage, apply_all};
use particlecode_render::{Anim, RenderConfig, render_anim};
use particlecode_vision::YuvFrame;

const DISPLAY: DisplayInfo = DisplayInfo {
    width_px: 1080,
    height_px: 2400,
    width_mm: 68.0,
    height_mm: 151.0,
};
const SIZE_PX: u32 = 760;
const PAD: usize = 8;
const SESSION_ID: u16 = 0xC0DE;
const KEY: [u8; 32] = [3; 32];
const ORIGIN: u64 = 1_700_000_000;

macro_rules! constants {
    ($($name:ident),* $(,)?) => {
        &[$((stringify!($name), $name as i64)),*]
    };
}

/// Every constant of the array layout and status codes, by name.
const CONSTANTS: &[(&str, i64)] = constants![
    STATUS_REJECTED,
    STATUS_ACCEPTED,
    STATUS_DONE,
    ERR_NULL,
    ERR_PANIC,
    ERR_INVALID,
    ERR_NOT_DIRECT,
    ERR_JNI,
    OUT_LEN,
    OUT_LOCKED,
    OUT_PROFILE,
    OUT_SHAPE,
    OUT_HAVE,
    OUT_NEED,
    OUT_SEGMENTS_DONE,
    OUT_SEGMENTS_TOTAL,
    OUT_LIVENESS_READ,
    OUT_LIVENESS_CONSISTENT,
    OUT_SEED_LOW,
    OUT_ROLLING,
    OUT_ANCHOR_COUNT,
    OUT_HINT,
    GEOM_LEN,
    GEOM_ANCHORS,
    GEOM_SHARPNESS,
    HINT_NONE,
    HINT_POINT_AT_CODE,
    HINT_MOVE_BACK,
    HINT_HOLD_STEADY,
    HINT_LIGHTING,
    HINT_UPDATE_APP,
    HINT_OTHER,
];

fn payload(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i * 31 % 251) as u8 ^ 0x5A).collect()
}

/// Render frame `i` mid-way through its 100 ms slot and photograph it.
fn camera_shot(session: &Session, i: u32) -> RgbImage {
    let frame = session.frame(i);
    let cfg = RenderConfig {
        size_px: SIZE_PX,
        ..RenderConfig::default()
    };
    let seed = seed_from_session(session.session_id());
    let anim = Anim::at(MotionConfig::default(), seed, f64::from(i) * 0.1 + 0.06);
    let img = RgbImage::from_pixmap(&render_anim(&frame, &cfg, &anim));
    apply_all(
        &img,
        &[
            Degradation::Tilt {
                pose: CameraPose {
                    rotate_deg: 8.0 + f64::from(i % 5) * 7.0,
                    tilt_x_deg: 12.0,
                    tilt_y_deg: -8.0,
                    scale: 0.85,
                },
                out_w: SIZE_PX * 5 / 4,
                out_h: SIZE_PX,
                background: Rgb(40, 38, 36),
            },
            Degradation::ColorCast {
                r: 1.06,
                g: 1.0,
                b: 0.9,
            },
            Degradation::Exposure {
                gain: 0.85,
                offset: 4.0,
            },
            Degradation::Blur { sigma: 0.9 },
            Degradation::Noise {
                sigma: 4.0,
                seed: u64::from(i),
            },
        ],
    )
}

/// Pad every row (`row_len` bytes) to `stride` with a recognizable filler.
fn pad_rows(data: &[u8], row_len: usize, stride: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / row_len * stride);
    for row in data.chunks(row_len) {
        out.extend_from_slice(row);
        out.resize(out.len() + (stride - row_len), 0xEE);
    }
    out
}

/// NV21 as Android hands it over: padded luma and one interleaved `VU` buffer.
struct Nv21 {
    width: usize,
    height: usize,
    y_stride: usize,
    uv_stride: usize,
    y: Vec<u8>,
    vu: Vec<u8>,
}

fn nv21(img: &RgbImage) -> Nv21 {
    let yuv =
        YuvFrame::from_rgb(img.width as usize, img.height as usize, &img.data).subsample_chroma();
    let (w, h) = (yuv.y.width, yuv.y.height);
    let cw = yuv.u.width;
    let mut vu = Vec::with_capacity(cw * yuv.u.height * 2);
    for (u, v) in yuv.u.data.iter().zip(&yuv.v.data) {
        vu.extend_from_slice(&[*v, *u]);
    }
    Nv21 {
        width: w,
        height: h,
        y_stride: w + PAD,
        uv_stride: cw * 2 + PAD,
        y: pad_rows(&yuv.y.data, w, w + PAD),
        vu: pad_rows(&vu, cw * 2, cw * 2 + PAD),
    }
}

/// Write the frames, the payload and the metadata the Java driver reads.
fn write_dataset(dir: &Path) {
    let data = payload(2600);
    let session = Session::with_identity(
        &data,
        DISPLAY,
        &SessionConfig::default(),
        SESSION_ID,
        KEY,
        ORIGIN,
    )
    .expect("session");
    // Enough frames to finish even when some are lost; the driver stops at `Done`.
    let mut indices = Vec::new();
    let mut shape = None;
    for i in 0..session.frames_per_pass() * 3 {
        // Drop every fourth frame: the fountain makes up for it.
        if i % 4 == 3 {
            continue;
        }
        let p = nv21(&camera_shot(&session, i));
        fs::write(dir.join(format!("frame_{i}.y")), &p.y).expect("write y");
        fs::write(dir.join(format!("frame_{i}.vu")), &p.vu).expect("write vu");
        shape = Some((p.width, p.height, p.y_stride, p.uv_stride));
        indices.push(i.to_string());
    }
    let (width, height, y_stride, uv_stride) = shape.expect("frames");
    fs::write(dir.join("payload.bin"), &data).expect("write payload");
    fs::write(dir.join("key.bin"), KEY).expect("write key");
    let meta = format!(
        "width={width}\nheight={height}\nyStride={y_stride}\nuvStride={uv_stride}\n\
         uvPixelStride=2\norigin={ORIGIN}\nsessionId={SESSION_ID}\nfps=10\nindices={}\n",
        indices.join(",")
    );
    fs::write(dir.join("meta.properties"), meta).expect("write meta");
}

/// `Layout.java`: the Rust constants, so the Java driver indexes the arrays exactly as the
/// library lays them out.
fn layout_java() -> String {
    let mut s = String::from("package com.vaam.particlecode;\n\nfinal class Layout {\n");
    s.push_str("    private Layout() {}\n");
    for (name, value) in CONSTANTS {
        writeln!(s, "    static final int {name} = {value};").expect("string write");
    }
    s.push_str("}\n");
    s
}

fn tool_works(tool: &str) -> bool {
    Command::new(tool)
        .arg("-version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// The profile directory of the running test binary (`target/debug`).
fn profile_dir() -> PathBuf {
    let exe = std::env::current_exe().expect("test exe path");
    // <target>/<profile>/deps/<test binary>
    exe.parent()
        .and_then(Path::parent)
        .expect("profile dir")
        .to_path_buf()
}

/// Build (or find) the cdylib with the profile the tests use, so the decoder is optimized
/// the same way as in the unit tests.
fn cdylib() -> PathBuf {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .args(["build", "-p", "particlecode-jni", "--profile", "test"])
        .status()
        .expect("run cargo build for the cdylib");
    assert!(status.success(), "cargo build of the cdylib failed");
    let name = format!(
        "{}particlecode_jni{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    let path = profile_dir().join(name);
    assert!(path.exists(), "cdylib not found at {}", path.display());
    path
}

fn run(cmd: &mut Command) -> (bool, String) {
    let out = cmd.output().expect("spawn");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

#[test]
fn jvm_drives_a_transfer_through_direct_buffers() {
    if !tool_works("javac") || !tool_works("java") {
        eprintln!("skipping jvm test: no JDK (javac/java) on PATH");
        return;
    }
    let work =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("jvm-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    let data_dir = work.join("data");
    let classes = work.join("classes");
    let gen_dir = work.join("gen/com/vaam/particlecode");
    for d in [&data_dir, &classes, &gen_dir] {
        fs::create_dir_all(d).expect("mkdir");
    }

    write_dataset(&data_dir);
    fs::write(gen_dir.join("Layout.java"), layout_java()).expect("write Layout.java");
    let lib = cdylib();

    let java_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/java/com/vaam/particlecode");
    let (ok, text) = run(Command::new("javac")
        .arg("-Xlint:all")
        .arg("-d")
        .arg(&classes)
        .arg(java_src.join("NativeBridge.java"))
        .arg(java_src.join("Driver.java"))
        .arg(gen_dir.join("Layout.java")));
    assert!(ok, "javac failed:\n{text}");

    let (ok, text) = run(Command::new("java")
        .arg("-Xcheck:jni")
        .arg(format!("-Dparticlecode.jni={}", lib.display()))
        .arg("-cp")
        .arg(&classes)
        .arg("com.vaam.particlecode.Driver")
        .arg(&data_dir));
    println!("{text}");
    assert!(ok, "java driver failed:\n{text}");
    assert!(text.contains("\nOK "), "driver did not finish:\n{text}");
    assert!(
        !text.contains("WARNING in native method") && !text.contains("FATAL ERROR"),
        "-Xcheck:jni complained:\n{text}"
    );
    let _ = fs::remove_dir_all(&work);
}

/// The Kotlin `const val`s must equal the Rust constants (the arrays' layout is a contract).
#[test]
fn kotlin_constants_match_the_rust_layout() {
    let kt = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../android/particlecode/src/main/kotlin/com/vaam/particlecode/NativeBridge.kt");
    let src = fs::read_to_string(&kt).unwrap_or_else(|e| panic!("read {}: {e}", kt.display()));
    let mut found = std::collections::BTreeMap::new();
    for line in src.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("const val ") else {
            continue;
        };
        let Some((name, value)) = rest.split_once('=') else {
            continue;
        };
        let name = name.split(':').next().unwrap_or("").trim();
        if let Ok(v) = value.trim().parse::<i64>() {
            found.insert(name.to_owned(), v);
        }
    }
    for (name, value) in CONSTANTS {
        assert_eq!(
            found.get(*name),
            Some(value),
            "Kotlin NativeBridge.{name} is missing or differs from Rust"
        );
    }
}
