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
    /// The command is specified (ADR-0007) but its implementation has not
    /// landed yet.
    #[error("not yet implemented: {0}")]
    NotYetImplemented(&'static str),

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
