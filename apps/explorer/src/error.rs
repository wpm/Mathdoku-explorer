//! Error model for Explorer.
//!
//! Theory: every failure surfaced to the user is a variant of one [`Error`]
//! enum (via `thiserror`, as in `mathdoku-designer-core`). Commands return
//! `Result<(), Error>`; `main` renders the error on the injected stderr
//! writer and maps it to a failing exit code. No code path panics.

use std::io;

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
}
