//! Command dispatch.
//!
//! Theory: [`run`] is the single entry point between the parsed command line
//! and the rest of the program. It receives injected stdout/stderr writers —
//! user-facing results go to the stdout writer, progress to the stderr
//! writer — so every command is unit-testable against in-memory buffers and
//! the crate satisfies the `print_stdout`/`print_stderr` denies. The
//! experiment registry is likewise a parameter of the inner
//! [`run_with_registry`], so tests drive the full dispatch path with fake
//! experiments while `run` itself binds the real
//! [`crate::experiments::registry`].

use std::io::Write;

use crate::cli::{Cli, Command};
use crate::error::Error;
use crate::experiments::{Experiment, registry};
use crate::runner;

/// Executes the parsed command against the real experiment registry,
/// writing results to `stdout` and progress to `stderr`.
///
/// # Errors
///
/// Returns [`Error`] when the command fails; the caller is responsible for
/// reporting it and choosing the exit code.
pub fn run(cli: Cli, stdout: &mut impl Write, stderr: &mut impl Write) -> Result<(), Error> {
    run_with_registry(cli, &registry(), stdout, stderr)
}

/// Executes the parsed command against an injected `registry`.
///
/// # Errors
///
/// Returns [`Error`] when the command fails.
pub fn run_with_registry(
    cli: Cli,
    registry: &[Box<dyn Experiment>],
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<(), Error> {
    match cli.command {
        Command::Perf {
            config,
            allow_debug,
        } => {
            let _ = runner::perf(registry, &config, allow_debug, stdout, stderr)?;
            Ok(())
        }
        Command::List => list(registry, stdout),
    }
}

/// Writes one `name  description` line per registered experiment, or a
/// note that the registry is empty.
fn list(registry: &[Box<dyn Experiment>], stdout: &mut impl Write) -> Result<(), Error> {
    if registry.is_empty() {
        writeln!(stdout, "no experiments registered")?;
        return Ok(());
    }
    let width = registry
        .iter()
        .map(|experiment| experiment.name().len())
        .max()
        .unwrap_or(0);
    for experiment in registry {
        writeln!(
            stdout,
            "{:width$}  {}",
            experiment.name(),
            experiment.description()
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser as _;

    use super::{run, run_with_registry};
    use crate::cli::Cli;
    use crate::error::Error;
    use crate::experiments::Experiment;
    use crate::experiments::fake::FakeExperiment;
    use crate::testutil::unique_temp_dir;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("test arguments should parse")
    }

    fn fake_registry() -> Vec<Box<dyn Experiment>> {
        vec![Box::new(FakeExperiment::new(&["alpha"], 1))]
    }

    #[test]
    fn list_includes_the_registered_solve_time_experiment() {
        let cli = parse(&["mathdoku-explorer", "list"]);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        run(cli, &mut stdout, &mut stderr).expect("list should succeed");
        let stdout = String::from_utf8(stdout).expect("output should be UTF-8");
        assert!(
            stdout.lines().any(|line| line.starts_with("solve-time ")),
            "list should include solve-time, got:\n{stdout}"
        );
        assert!(stderr.is_empty());
    }

    #[test]
    fn list_reports_an_empty_registry() {
        let cli = parse(&["mathdoku-explorer", "list"]);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        run_with_registry(cli, &[], &mut stdout, &mut stderr).expect("list should succeed");
        assert_eq!(
            String::from_utf8(stdout).expect("output should be UTF-8"),
            "no experiments registered\n"
        );
        assert!(stderr.is_empty());
    }

    #[test]
    fn list_writes_names_and_descriptions() {
        let cli = parse(&["mathdoku-explorer", "list"]);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        run_with_registry(cli, &fake_registry(), &mut stdout, &mut stderr)
            .expect("list should succeed");
        assert_eq!(
            String::from_utf8(stdout).expect("output should be UTF-8"),
            "fake  a deterministic fake for tests\n"
        );
        assert!(stderr.is_empty());
    }

    #[test]
    fn perf_rejects_an_unknown_experiment_listing_known_names() {
        let directory = unique_temp_dir("app-unknown");
        let config_path = directory.join("experiment.yaml");
        std::fs::write(&config_path, "experiment: no-such-experiment\n")
            .expect("the config file should be writable");
        let cli = parse(&[
            "mathdoku-explorer",
            "perf",
            config_path.to_str().expect("the path should be UTF-8"),
            "--allow-debug",
        ]);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let result = run_with_registry(cli, &fake_registry(), &mut stdout, &mut stderr);
        std::fs::remove_dir_all(&directory).expect("the temp directory should be removable");
        match result {
            Err(Error::UnknownExperiment { name, known }) => {
                assert_eq!(name, "no-such-experiment");
                assert_eq!(known, vec!["fake".to_owned()]);
            }
            other => panic!("expected UnknownExperiment, got {other:?}"),
        }
        assert!(stdout.is_empty(), "no results should be written on failure");
    }

    #[test]
    fn unknown_experiment_error_message_names_the_alternatives() {
        let error = Error::UnknownExperiment {
            name: "nope".to_owned(),
            known: vec!["fake".to_owned(), "solve-time".to_owned()],
        };
        assert_eq!(
            error.to_string(),
            "unknown experiment `nope`; known experiments: fake, solve-time"
        );
    }

    #[test]
    fn unknown_experiment_error_message_handles_an_empty_registry() {
        let error = Error::UnknownExperiment {
            name: "nope".to_owned(),
            known: Vec::new(),
        };
        assert_eq!(
            error.to_string(),
            "unknown experiment `nope`; known experiments: (none registered)"
        );
    }

    #[test]
    fn perf_runs_a_fake_experiment_end_to_end() {
        let directory = unique_temp_dir("app-perf");
        let config_path = directory.join("experiment.yaml");
        let output_directory = directory.join("results");
        std::fs::write(
            &config_path,
            format!(
                "experiment: fake\nseed: 7\nwarmup: 1\nsamples: 3\noutput_directory: {}\n",
                output_directory.display()
            ),
        )
        .expect("the config file should be writable");
        let cli = parse(&[
            "mathdoku-explorer",
            "perf",
            config_path.to_str().expect("the path should be UTF-8"),
            "--allow-debug",
        ]);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let result = run_with_registry(cli, &fake_registry(), &mut stdout, &mut stderr);
        let stdout = String::from_utf8(stdout).expect("output should be UTF-8");
        let stderr = String::from_utf8(stderr).expect("progress should be UTF-8");
        let run_directories: Vec<_> = std::fs::read_dir(&output_directory)
            .expect("the output directory should exist")
            .collect::<Result<_, _>>()
            .expect("the output directory should be readable");
        std::fs::remove_dir_all(&directory).expect("the temp directory should be removable");
        result.expect("perf should succeed");
        assert_eq!(run_directories.len(), 1, "exactly one run directory");
        let run_directory = run_directories[0].path();
        assert!(
            stdout.trim_end().ends_with(
                run_directory
                    .to_str()
                    .expect("the run directory should be UTF-8")
            ),
            "stdout should end with the run directory path, got:\n{stdout}"
        );
        assert!(
            stderr.contains("condition c0"),
            "progress should mention the condition, got:\n{stderr}"
        );
    }
}
