//! Session to Receiver at the symbol level, with lost frames and corrupted symbols.
//!
//! A 30 KB payload is split into segments, every 4th frame is dropped and 1% of the
//! received symbols are replaced by wrong ones. The receiver's progress (symbols held
//! against symbols needed, segments completed) is printed as the segments stream in.
//! This is what the camera path will do, minus the pixels.
//!
//! ```sh
//! cargo run --release -p particlecode-cli --example symbol_roundtrip
//! ```
#![allow(clippy::print_stdout, clippy::print_stderr)] // examples report to the terminal

use anyhow::{Result, bail};
use particlecode_cli::roundtrip::{self, Options, Outcome};
use particlecode_cli::sample;
use particlecode_cli::spec::SessionSpec;

fn main() -> Result<()> {
    let payload = sample::credential(30_000);
    let spec = SessionSpec {
        // 4 KiB segments: the payload arrives as separately verified pieces.
        segment_kb_log2: 2,
        session_id: Some(0x5EED),
        key: Some([0x42; 32]),
        clock_origin: Some(0),
        ..SessionSpec::default()
    };
    let session = spec.build(&payload)?;
    println!(
        "{} bytes in {} segments, {} bytes per frame, {} frames per pass",
        payload.len(),
        session.packets().segment_count(),
        session.block_size(),
        session.frames_per_pass()
    );

    let opts = Options {
        drop_every: 4,
        error_rate: 0.01,
        seed: 2024,
        max_frames: 500,
    };
    let mut last_segments = 0;
    let report = roundtrip::run(&session, &payload, &opts, |step| {
        let bar = match &step.outcome {
            Outcome::Accepted { have, need, .. } => progress_bar(*have, *need),
            Outcome::Done => progress_bar(1, 1),
            _ => String::new(),
        };
        println!("{}  {bar}", roundtrip::describe(step));
        if let Outcome::Accepted { segments_done, .. } = &step.outcome
            && *segments_done != last_segments
        {
            last_segments = *segments_done;
            println!("    -> segment {segments_done} verified and delivered");
        }
    });

    println!(
        "\nshown {} frames ({} dropped, {} rejected), {} symbols corrupted; a perfect channel needs {}",
        report.frames_shown,
        report.frames_dropped,
        report.frames_rejected,
        report.symbols_corrupted,
        report.frames_minimum
    );
    if !report.success {
        bail!("payload was not recovered");
    }
    println!("payload recovered intact ({} bytes)", payload.len());
    Ok(())
}

fn progress_bar(have: u32, need: u32) -> String {
    const WIDTH: u32 = 30;
    let filled = (have * WIDTH / need.max(1)).min(WIDTH);
    format!(
        "[{}{}]",
        "#".repeat(filled as usize),
        "-".repeat((WIDTH - filled) as usize)
    )
}
