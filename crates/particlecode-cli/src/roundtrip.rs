//! `roundtrip`: symbol-level self-test. A [`Session`] produces frames, a seeded channel
//! drops some and corrupts symbols in the rest, and a [`Receiver`] reassembles the payload.
//!
//! The channel works on the exact symbols a vision stage would hand over
//! ([`FrameSpec::all_symbols`](particlecode_core::FrameSpec::all_symbols)), so it exercises
//! the header code, Reed-Solomon, interleaving and the fountain decoder, but not the camera.
//! An image-level version comes with the vision crate.

use std::io::Write;

use anyhow::{Result, bail};
use particlecode_core::{DecodeError, Progress, Receiver, Session, Sym3};
use particlecode_render::synth::Rng;

use crate::cli::RoundtripArgs;
use crate::encode::shape_choice;
use crate::spec::SessionSpec;

/// The simulated channel and the stop condition.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    /// Drop every N-th frame (0: none).
    pub drop_every: u32,
    /// Probability that a symbol arrives as a different, wrong one.
    pub error_rate: f64,
    /// Seed of the channel.
    pub seed: u64,
    /// Give up after this many frames shown.
    pub max_frames: u32,
}

/// What happened to one displayed frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The channel lost the frame.
    Dropped,
    /// The receiver refused it (for example too many symbol errors).
    Rejected(DecodeError),
    /// The receiver used it.
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
    /// The payload is complete.
    Done,
}

/// One displayed frame and its fate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// Frame index in the session.
    pub index: u32,
    /// What the receiver did with it.
    pub outcome: Outcome,
    /// Symbols the channel corrupted in this frame.
    pub symbols_corrupted: usize,
}

/// Summary of a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// Frames shown (dropped ones included).
    pub frames_shown: u32,
    /// Frames the channel dropped.
    pub frames_dropped: u32,
    /// Frames the receiver rejected.
    pub frames_rejected: u32,
    /// Frames the receiver accepted (the completing frame included).
    pub frames_accepted: u32,
    /// Symbols the channel corrupted in frames that were not dropped.
    pub symbols_corrupted: u64,
    /// Frames needed with a perfect channel: `Σ K_s`.
    pub frames_minimum: u32,
    /// The payload came back and equals the original.
    pub success: bool,
}

/// Run the session through the channel into a fresh receiver, calling `on_step` after every
/// frame. Stops when the payload is complete or `opts.max_frames` frames were shown.
pub fn run(
    session: &Session,
    payload: &[u8],
    opts: &Options,
    mut on_step: impl FnMut(&Step),
) -> Report {
    let packets = session.packets();
    let frames_minimum = (0..packets.segment_count())
        .filter_map(|s| packets.source_symbols(s))
        .sum();
    let mut report = Report {
        frames_shown: 0,
        frames_dropped: 0,
        frames_rejected: 0,
        frames_accepted: 0,
        symbols_corrupted: 0,
        frames_minimum,
        success: false,
    };
    let mut rng = Rng::new(opts.seed);
    let mut receiver = Receiver::new();

    for index in 0..opts.max_frames {
        report.frames_shown += 1;
        if opts.drop_every > 0 && (index + 1) % opts.drop_every == 0 {
            report.frames_dropped += 1;
            on_step(&Step {
                index,
                outcome: Outcome::Dropped,
                symbols_corrupted: 0,
            });
            continue;
        }
        let mut symbols = session.frame(index).all_symbols();
        let corrupted = corrupt(&mut symbols, opts.error_rate, &mut rng);
        report.symbols_corrupted += corrupted as u64;
        let outcome = match receiver.feed(&symbols) {
            Progress::Rejected(e) => {
                report.frames_rejected += 1;
                Outcome::Rejected(e)
            }
            Progress::Accepted {
                have,
                need,
                segments_done,
                segments_total,
            } => {
                report.frames_accepted += 1;
                Outcome::Accepted {
                    have,
                    need,
                    segments_done,
                    segments_total,
                }
            }
            Progress::Done(data) => {
                report.frames_accepted += 1;
                report.success = data == payload;
                Outcome::Done
            }
        };
        let done = outcome == Outcome::Done;
        on_step(&Step {
            index,
            outcome,
            symbols_corrupted: corrupted,
        });
        if done {
            break;
        }
    }
    report
}

/// Replace each symbol by a different random value with probability `rate`; returns how
/// many were replaced.
fn corrupt(symbols: &mut [Sym3], rate: f64, rng: &mut Rng) -> usize {
    if rate <= 0.0 {
        return 0;
    }
    let mut n = 0;
    for s in symbols {
        if rng.next_f64() < rate {
            let shift = 1 + rng.below(7) as u8;
            *s = Sym3::new((s.value + shift) & 7);
            n += 1;
        }
    }
    n
}

/// Execute the `roundtrip` subcommand: print the report and fail if the payload did not
/// come back.
pub fn command(args: &RoundtripArgs, payload: &[u8], out: &mut impl Write) -> Result<()> {
    let spec = SessionSpec {
        profile: args.profile.into(),
        shape: shape_choice(args.shape, &[]),
        is_static: args.is_static,
        segment_kb_log2: args.segment_kb_log2,
        session_id: Some(0x5EED),
        key: Some([0x42; 32]),
        clock_origin: Some(0),
        ..SessionSpec::default()
    };
    let session = spec.build(payload)?;
    let per_pass = session.frames_per_pass();
    let opts = Options {
        drop_every: args.drop_every,
        error_rate: args.error_rate,
        seed: args.seed,
        max_frames: args
            .max_frames
            .unwrap_or_else(|| per_pass.saturating_mul(10).max(50)),
    };
    writeln!(
        out,
        "profile {:?}, {} dots, {} bytes/frame, {} segment(s), {} frames/pass",
        session.profile(),
        session.dot_order().len(),
        session.block_size(),
        session.packets().segment_count(),
        per_pass
    )?;
    let mut io_error = None;
    let report = run(&session, payload, &opts, |step| {
        if args.verbose && io_error.is_none() {
            io_error = writeln!(out, "{}", describe(step)).err();
        }
    });
    if let Some(e) = io_error {
        return Err(e.into());
    }
    writeln!(
        out,
        "shown {} frames: {} dropped, {} rejected, {} accepted; {} symbols corrupted",
        report.frames_shown,
        report.frames_dropped,
        report.frames_rejected,
        report.frames_accepted,
        report.symbols_corrupted
    )?;
    if report.success {
        writeln!(
            out,
            "OK: {} bytes recovered after {} frames (minimum {})",
            payload.len(),
            report.frames_shown,
            report.frames_minimum
        )?;
        Ok(())
    } else {
        bail!(
            "payload not recovered within {} frames (minimum {})",
            opts.max_frames,
            report.frames_minimum
        )
    }
}

/// One-line description of a step, for `--verbose` and the example.
pub fn describe(step: &Step) -> String {
    let what = match &step.outcome {
        Outcome::Dropped => "dropped".to_owned(),
        Outcome::Rejected(e) => format!("rejected ({e})"),
        Outcome::Accepted {
            have,
            need,
            segments_done,
            segments_total,
        } => format!("have {have}/{need}, segments {segments_done}/{segments_total}"),
        Outcome::Done => "done".to_owned(),
    };
    format!(
        "frame {:>4}: {} ({} symbols hit)",
        step.index, what, step.symbols_corrupted
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i * 7 + 3) as u8).collect()
    }

    fn session(data: &[u8]) -> Session {
        SessionSpec {
            session_id: Some(9),
            key: Some([1; 32]),
            clock_origin: Some(0),
            ..SessionSpec::default()
        }
        .build(data)
        .expect("session")
    }

    fn opts() -> Options {
        Options {
            drop_every: 0,
            error_rate: 0.0,
            seed: 5,
            max_frames: 200,
        }
    }

    #[test]
    fn clean_channel_needs_the_minimum() {
        let data = payload(5000);
        let s = session(&data);
        let r = run(&s, &data, &opts(), |_| {});
        assert!(r.success);
        assert_eq!(r.frames_shown, r.frames_minimum);
        assert_eq!(r.frames_dropped + r.frames_rejected, 0);
    }

    #[test]
    fn dropped_frames_cost_repair_frames() {
        let data = payload(5000);
        let s = session(&data);
        let r = run(
            &s,
            &data,
            &Options {
                drop_every: 4,
                ..opts()
            },
            |_| {},
        );
        assert!(r.success);
        assert!(r.frames_dropped > 0);
        assert!(r.frames_shown > r.frames_minimum);
    }

    #[test]
    fn light_symbol_errors_are_corrected() {
        let data = payload(5000);
        let s = session(&data);
        let r = run(
            &s,
            &data,
            &Options {
                error_rate: 0.01,
                ..opts()
            },
            |_| {},
        );
        assert!(r.success);
        assert!(r.symbols_corrupted > 0);
        assert_eq!(r.frames_rejected, 0, "1% symbol errors are within the ECC");
    }

    #[test]
    fn heavy_errors_fail_cleanly() {
        let data = payload(2000);
        let s = session(&data);
        let r = run(
            &s,
            &data,
            &Options {
                error_rate: 0.5,
                max_frames: 20,
                ..opts()
            },
            |_| {},
        );
        assert!(!r.success);
        assert_eq!(r.frames_shown, 20);
        assert_eq!(r.frames_rejected, 20);
    }

    #[test]
    fn steps_are_reported_in_order() {
        let data = payload(1500);
        let s = session(&data);
        let mut seen = Vec::new();
        run(&s, &data, &opts(), |step| seen.push(step.clone()));
        assert!(
            seen.iter()
                .enumerate()
                .all(|(i, st)| st.index as usize == i)
        );
        assert_eq!(seen.last().map(|s| &s.outcome), Some(&Outcome::Done));
        assert!(describe(&seen[0]).contains("frame"));
    }

    #[test]
    fn corruption_changes_exactly_the_hit_symbols() {
        let mut rng = Rng::new(3);
        let mut syms = vec![Sym3::new(0); 1000];
        let n = corrupt(&mut syms, 0.1, &mut rng);
        assert_eq!(syms.iter().filter(|s| s.value != 0).count(), n);
        assert!(n > 40 && n < 200);
        assert_eq!(corrupt(&mut syms, 0.0, &mut rng), 0);
    }

    #[test]
    fn command_reports_success_and_failure() {
        use crate::cli::{Cli, Command};
        use clap::Parser as _;
        let parse = |extra: &[&str]| match Cli::try_parse_from(
            ["particlecode", "roundtrip", "-"]
                .into_iter()
                .chain(extra.iter().copied()),
        )
        .expect("parses")
        .command
        {
            Command::Roundtrip(a) => a,
            _ => panic!("not roundtrip"),
        };
        let data = payload(3000);
        let mut out = Vec::new();
        command(
            &parse(&["--drop-every", "5", "--error-rate", "0.01"]),
            &data,
            &mut out,
        )
        .expect("succeeds");
        let text = String::from_utf8(out).expect("utf8");
        assert!(text.contains("OK:"), "{text}");

        let mut out = Vec::new();
        let err = command(
            &parse(&["--error-rate", "0.6", "--max-frames", "10"]),
            &data,
            &mut out,
        )
        .expect_err("hopeless channel");
        assert!(err.to_string().contains("not recovered"));
    }
}
