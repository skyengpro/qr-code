//! The whole pipeline in one process: payload, session, animated render, synthetic phone
//! camera, vision decoder, receiver, original bytes.
//!
//! Each frame is drawn with the breathing wave, photographed through a tilt, a warm colour
//! cast, blur and sensor noise (`particlecode_render::synth`), then located and read by
//! `particlecode-vision` and fed to a `Receiver`. Every third frame is lost on the way.
//! The particles carry a rolling HMAC of the clock; since this example knows the session
//! key it also verifies that value, which is what a verifier with the key would do.
//!
//! ```sh
//! cargo run --release -p particlecode-cli --example camera_roundtrip
//! ```
#![allow(clippy::print_stdout, clippy::print_stderr)] // examples report to the terminal

use anyhow::{Context, Result, bail};
use particlecode_cli::camera::{self, Camera, Event};
use particlecode_cli::decode::Outcome;
use particlecode_cli::spec::SessionSpec;
use particlecode_core::{Liveness, liveness};

const KEY: [u8; 32] = [0x42; 32];
const CLOCK_ORIGIN: u64 = 1_780_000_000;
const FPS: u16 = 10;

fn main() -> Result<()> {
    let payload = std::fs::read("examples/data/credential.cbor")
        .context("run this example from the repository root")?;
    let session = SessionSpec {
        session_id: Some(0xCA3E),
        key: Some(KEY),
        clock_origin: Some(CLOCK_ORIGIN),
        fps: FPS,
        ..SessionSpec::default()
    }
    .build(&payload)?;
    let camera = Camera::default();
    println!(
        "{} payload bytes -> profile {:?}, {} bytes per frame, {} frames per pass",
        payload.len(),
        session.profile(),
        session.block_size(),
        session.frames_per_pass()
    );
    println!(
        "camera: blur {} px, noise {}, tilt {} deg; every 3rd frame is lost\n",
        camera.blur, camera.noise, camera.tilt
    );

    let report = camera::run(&session, &payload, &camera, 3, 40, |step, decoder| {
        let mut line = camera::describe(step, decoder);
        if let Event::Frame(fr) = &step.event
            && let Liveness::Read { rolling, .. } = fr.liveness
            && matches!(fr.outcome, Outcome::Accepted { .. } | Outcome::Done)
        {
            // The sender's clock for frame `i` is `origin + i / fps`.
            let now = CLOCK_ORIGIN + u64::from(step.index / u32::from(FPS));
            let ok = liveness::verify(&KEY, rolling, now, 1);
            line.push_str(if ok {
                "; HMAC verified"
            } else {
                "; HMAC MISMATCH"
            });
        }
        println!("{line}");
    });

    println!(
        "\n{} frames shown: {} dropped, {} lock failures, {} unreadable, {} RS rejections, \
         {} accepted (a perfect channel needs {})",
        report.frames_shown,
        report.frames_dropped,
        report.lock_failures,
        report.read_failures,
        report.rs_rejections,
        report.frames_accepted,
        report.frames_minimum
    );
    if !report.success {
        bail!("payload was not recovered");
    }
    println!(
        "payload verified: {} bytes identical to the input",
        payload.len()
    );
    Ok(())
}
