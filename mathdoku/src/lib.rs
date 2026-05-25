//! Mathdoku puzzle generator and solver.
//!
//! ## Core types
//!
//! | Type | Role |
//! |------|------|
//! | [`Cell`] | A grid position identified by `(row, column)`. |
//! | [`Values`] | A bitmap set of candidate values `1..=9` for a cell. |
//! | [`Cage`] | A polyomino paired with an [`Operation`]. |
//! | [`puzzle::Puzzle`] | An `n×n` grid with a set of cages. |
//!
//! ## Entry points
//!
//! - **Generate** a random puzzle with [`generate::generate`] or
//!   [`generate::generate_with`] (custom operation policy / cage-size distribution).
//! - **Construct** a puzzle programmatically with [`puzzle::Puzzle::new`] and
//!   [`puzzle::Puzzle::insert_cage`].
//! - **Inspect** cell domains with [`puzzle::Puzzle::get_cell_values`].

#![allow(
    clippy::must_use_candidate,
    clippy::return_self_not_must_use,
    clippy::expect_used,
    clippy::missing_panics_doc
)]

mod arithmetic;
mod cage;
mod cell;
mod csp;
mod error;
pub mod generate;
mod latin_square;
mod polyomino;
pub mod puzzle;
mod puzzle_csp;
mod regin;
#[cfg(test)]
mod test_utils;

pub use cage::{Cage, Operation, Operator};
pub use cell::{Cell, M, N, Values};
pub use error::Error;
pub use polyomino::Polyomino;
