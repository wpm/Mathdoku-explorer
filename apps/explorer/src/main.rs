//! Mathdoku Explorer: a grab-bag of scientific experiments over the mathdoku
//! library (ADR-0007).
//!
//! Theory: `main` is a thin shim. It parses the command line, hands the
//! parsed [`cli::Cli`] to [`app::run`] together with locked stdout/stderr
//! handles, and maps the result onto a process exit code. All user-facing
//! output — including error reports — flows through injected
//! [`std::io::Write`] handles; nothing in this crate prints directly.

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::print_stderr
    )
)]

mod app;
mod cli;
mod config;
mod error;
mod experiments;
mod output;
mod runner;
mod stats;
#[cfg(test)]
mod testutil;

use std::io::Write as _;
use std::process::ExitCode;

use clap::Parser as _;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    match app::run(cli, &mut stdout, &mut stderr) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // If even the error report cannot be written, the failing exit
            // code is the only signal left to give.
            let _ = writeln!(stderr, "error: {error}");
            ExitCode::FAILURE
        }
    }
}
