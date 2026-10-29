//! # particlecode-cli
//!
//! The `particlecode` command line and the helpers shared with the runnable examples in
//! the top-level `examples/` folder.
//!
//! | Module | Purpose |
//! |--------|---------|
//! | [`cli`] | clap definition and value parsers |
//! | [`encode`] | `encode`: payload to PNG / GIF / SVG / PNG sequence |
//! | [`info`] | `info`: capacity tables |
//! | [`roundtrip`] | `roundtrip`: symbol-level self-test through a lossy channel |
//! | [`decode`] | `decode`: PNG images to payload; [`decode::Decoder`] is the image-level receiver |
//! | [`camera`] | synthetic camera (render, degrade, decode) behind `roundtrip --camera` |
//! | [`spec`] | [`SessionSpec`](spec::SessionSpec): the session knobs the commands share |
//! | [`sample`] | sample CBOR payloads |
//! | [`sheet`] | contact sheets |
//!
//! [`run`] is the whole program minus argument parsing and process exit, so it can be
//! tested with in-memory writers.

pub mod camera;
pub mod cli;
pub mod decode;
pub mod encode;
pub mod info;
pub mod roundtrip;
pub mod sample;
pub mod sheet;
pub mod spec;

use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Context, Result};
use particlecode_core::Profile;

use cli::{Cli, Command};

/// Read the payload from `path`, or from standard input when `path` is `-`.
pub fn read_payload(path: &Path) -> Result<Vec<u8>> {
    if path == Path::new("-") {
        let mut buf = Vec::new();
        std::io::stdin()
            .lock()
            .read_to_end(&mut buf)
            .context("cannot read standard input")?;
        Ok(buf)
    } else {
        std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))
    }
}

/// Execute a parsed command line. Results go to `stdout`, progress and summaries to
/// `stderr`.
pub fn run(cli: &Cli, stdout: &mut impl Write, stderr: &mut impl Write) -> Result<()> {
    match &cli.command {
        Command::Encode(args) => {
            let payload = read_payload(&args.input)?;
            let summary = encode::run(args, &payload)?;
            writeln!(stderr, "{summary}")?;
        }
        Command::Info(args) => {
            let profiles: Vec<Profile> = if args.profile.is_empty() {
                Profile::ALL.to_vec()
            } else {
                args.profile.iter().map(|&p| p.into()).collect()
            };
            let shapes = if args.shape.is_empty() {
                info::builtin_shape_ids()
            } else {
                args.shape.clone()
            };
            let rows = info::rows(&profiles, &shapes);
            anyhow::ensure!(!rows.is_empty(), "none of the requested shapes exists");
            info::write_table(stdout, &rows)?;
        }
        Command::Roundtrip(args) => {
            let payload = read_payload(&args.input)?;
            roundtrip::command(args, &payload, stdout)?;
        }
        Command::Decode(args) => {
            decode::command(args, decode::stdout_is_terminal(), stdout, stderr)?;
        }
    }
    Ok(())
}
