//! Command dispatch.
//!
//! Theory: [`run`] is the single entry point between the parsed command line
//! and the rest of the program. It receives injected stdout/stderr writers —
//! user-facing results go to the stdout writer, progress (later) to the
//! stderr writer — so every command is unit-testable against in-memory
//! buffers and the crate satisfies the `print_stdout`/`print_stderr` denies.

use std::io::Write;

use crate::cli::{Cli, Command};
use crate::error::Error;

/// Executes the parsed command, writing results to `stdout` and progress to
/// `stderr`.
///
/// # Errors
///
/// Returns [`Error`] when the command fails; the caller is responsible for
/// reporting it and choosing the exit code.
// No command writes progress yet, but the stderr handle is part of the
// `run` contract (ADR-0007) so the test bed can stream progress in a later
// PR without changing any caller.
#[allow(clippy::needless_pass_by_ref_mut)]
pub fn run(cli: Cli, stdout: &mut impl Write, stderr: &mut impl Write) -> Result<(), Error> {
    let _ = stderr;
    match cli.command {
        Command::Perf { config } => {
            // The performance test bed (ADR-0007) lands in a later PR; until
            // then the parsed path is acknowledged and discarded.
            drop(config);
            Err(Error::NotYetImplemented("perf"))
        }
        Command::List => {
            // The experiment registry arrives with the test bed.
            writeln!(stdout, "no experiments registered yet")?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser as _;

    use super::run;
    use crate::cli::Cli;
    use crate::error::Error;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("test arguments should parse")
    }

    #[test]
    fn list_reports_an_empty_registry() {
        let cli = parse(&["mathdoku-explorer", "list"]);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        run(cli, &mut stdout, &mut stderr).expect("list should succeed");
        assert_eq!(
            String::from_utf8(stdout).expect("output should be UTF-8"),
            "no experiments registered yet\n"
        );
        assert!(stderr.is_empty());
    }

    #[test]
    fn perf_is_not_yet_implemented() {
        let cli = parse(&["mathdoku-explorer", "perf", "experiment.yaml"]);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let result = run(cli, &mut stdout, &mut stderr);
        assert!(matches!(result, Err(Error::NotYetImplemented("perf"))));
        assert!(stdout.is_empty());
    }
}
