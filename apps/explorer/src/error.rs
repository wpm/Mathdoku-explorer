//! Error model for Explorer.
//!
//! Theory: every failure surfaced to the user is a variant of one [`Error`]
//! enum (via `thiserror`, as in `mathdoku-designer-core`). Commands return
//! `Result<(), Error>`; `main` renders the error on the injected stderr
//! writer and maps it to a failing exit code. No code path panics.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Everything that can go wrong while running an Explorer command.
#[derive(Debug, Error)]
pub enum Error {
    /// The `perf` configuration names an experiment that is not in the
    /// registry.
    #[error(
        "unknown experiment `{name}`; known experiments: {}",
        known_names(known)
    )]
    UnknownExperiment {
        /// The unrecognised experiment name from the configuration.
        name: String,
        /// Every registered experiment name, for the user to pick from.
        known: Vec<String>,
    },

    /// The binary was compiled with `debug_assertions`: timings of an
    /// unoptimised build say nothing about the optimised library, so the
    /// runner refuses unless explicitly overridden.
    #[error(
        "refusing to measure a debug build: unoptimised timings are \
         meaningless; rebuild with --release, or pass --allow-debug to \
         measure anyway"
    )]
    DebugBuild,

    /// An experiment declared the runner's reserved derived-total phase
    /// name as one of its own phases.
    #[error(
        "experiment `{experiment}` declares the phase name `{phase}`, \
         which is reserved for the runner's derived per-trial total"
    )]
    ReservedPhase {
        /// The offending experiment.
        experiment: String,
        /// The reserved phase name, [`crate::runner::TOTAL_PHASE`].
        phase: &'static str,
    },

    /// An experiment declared the same phase name more than once.
    #[error("experiment `{experiment}` declares the phase `{phase}` more than once")]
    DuplicatePhase {
        /// The offending experiment.
        experiment: String,
        /// The repeated phase name.
        phase: &'static str,
    },

    /// An experiment returned measurements whose phases do not match the
    /// phases it declared up front.
    #[error(
        "experiment `{experiment}` returned measurement phases [{actual}] \
         but declared [{expected}]"
    )]
    PhaseMismatch {
        /// The offending experiment.
        experiment: String,
        /// The declared phases, comma-separated.
        expected: String,
        /// The phases actually returned, comma-separated.
        actual: String,
    },

    /// A set of measurements could not be summarised statistically.
    #[error("cannot summarise measurements: {source}")]
    Stats {
        /// The underlying statistics error.
        #[from]
        source: crate::stats::Error,
    },

    /// A CSV output file could not be written.
    #[error("cannot write CSV output: {source}")]
    Csv {
        /// The underlying CSV error.
        #[from]
        source: csv::Error,
    },

    /// The run output directory could not be created.
    #[error("cannot create run output directory under `{path}`: {source}")]
    OutputDir {
        /// The parent directory under which creation failed.
        path: PathBuf,
        /// The underlying I/O error.
        source: io::Error,
    },

    /// A run output file's YAML content could not be serialized.
    #[error("cannot serialize run output file `{file}`: {source}")]
    OutputSerialize {
        /// The run-directory file whose content failed to serialize.
        file: &'static str,
        /// The underlying YAML serialization error.
        source: serde_yaml_ng::Error,
    },

    /// A run output file could not be written.
    #[error("cannot write output file `{path}`: {source}")]
    OutputWrite {
        /// The file that could not be written.
        path: PathBuf,
        /// The underlying I/O error.
        source: io::Error,
    },

    /// An I/O failure, e.g. while writing to an output handle.
    #[error(transparent)]
    Io(#[from] io::Error),

    /// An experiment configuration could not be parsed as YAML matching the
    /// [`crate::config::ExperimentConfig`] schema (this includes unknown
    /// fields, which are rejected so that typos fail loudly).
    #[error("cannot parse experiment configuration: {source}")]
    ConfigParse {
        /// The underlying YAML parse or schema error.
        #[from]
        source: serde_yaml_ng::Error,
    },

    /// An experiment configuration file could not be opened for reading.
    #[error("cannot read experiment configuration file `{path}`: {source}")]
    ConfigRead {
        /// The configuration file that could not be opened.
        path: PathBuf,
        /// The underlying I/O error.
        source: io::Error,
    },

    /// An experiment configuration parsed, but a field violates the
    /// statistical protocol's invariants.
    #[error("invalid experiment configuration: `{field}` {reason}")]
    InvalidConfig {
        /// The offending configuration field.
        field: &'static str,
        /// Why the field's value is unacceptable.
        reason: String,
    },

    /// A parse or validation failure inside a configuration file, annotated
    /// with the file's path.
    #[error("in experiment configuration file `{path}`: {source}")]
    ConfigFile {
        /// The configuration file that contains the error.
        path: PathBuf,
        /// The underlying parse or validation error.
        #[source]
        source: Box<Self>,
    },
}

/// Renders the registry's experiment names for [`Error::UnknownExperiment`].
fn known_names(known: &[String]) -> String {
    if known.is_empty() {
        "(none registered)".to_owned()
    } else {
        known.join(", ")
    }
}
