//! `particlecode` binary: argument parsing and exit codes only; see the library for logic.

use std::io::Write;
use std::process::ExitCode;

use clap::Parser;
use particlecode_cli::cli::Cli;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    match particlecode_cli::run(&cli, &mut stdout, &mut stderr) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // Nothing sensible to do if stderr itself is gone.
            let _ = writeln!(stderr, "error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
