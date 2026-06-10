//! Command-line surface of Explorer.
//!
//! Theory: Explorer is structured like `git` — one binary, many subcommands,
//! each subcommand an experiment or a query over the registry (ADR-0007).
//! This module is purely declarative: the clap derive types *are* the
//! interface specification, and nothing here performs work or I/O.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// A grab-bag of scientific experiments over the mathdoku library.
#[derive(Debug, Parser)]
#[command(name = "mathdoku-explorer", version)]
pub struct Cli {
    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// The Explorer subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run a performance experiment from a YAML configuration file.
    Perf {
        /// Path to the YAML experiment configuration file.
        config: PathBuf,
    },
    /// List the available experiments.
    List,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::Parser as _;

    use super::{Cli, Command};

    #[test]
    fn perf_parses_config_path() {
        let cli = Cli::try_parse_from(["mathdoku-explorer", "perf", "experiment.yaml"])
            .expect("perf with a config path should parse");
        match cli.command {
            Command::Perf { config } => assert_eq!(config, PathBuf::from("experiment.yaml")),
            Command::List => panic!("expected the perf subcommand"),
        }
    }

    #[test]
    fn list_parses() {
        let cli = Cli::try_parse_from(["mathdoku-explorer", "list"]).expect("list should parse");
        assert!(matches!(cli.command, Command::List));
    }

    #[test]
    fn perf_without_config_fails() {
        assert!(Cli::try_parse_from(["mathdoku-explorer", "perf"]).is_err());
    }

    #[test]
    fn missing_subcommand_fails() {
        assert!(Cli::try_parse_from(["mathdoku-explorer"]).is_err());
    }

    #[test]
    fn unknown_subcommand_fails() {
        assert!(Cli::try_parse_from(["mathdoku-explorer", "frobnicate"]).is_err());
    }
}
