//! `particlecode-shapes`: the shape tool binary. All logic lives in the library.
// Reports are the program's output; the library writes to a `Write`, `main` only chooses
// stdout and the exit code.
#![allow(clippy::print_stderr)] // reason: top-level error reporting

use std::io::Write as _;
use std::process::ExitCode;

use clap::Parser;
use particlecode_shapes::cli::{Cli, Outcome, run};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut stdout = std::io::stdout().lock();
    let result = run(cli, &mut stdout);
    let _ = stdout.flush();
    match result {
        Ok(Outcome::Success) => ExitCode::SUCCESS,
        Ok(Outcome::RulesFailed) => ExitCode::from(1),
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(1)
        }
    }
}
