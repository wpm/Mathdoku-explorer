//! Mathdoku puzzle generator and solver.
//!
//! ## Core types
//!
//! | Type | Role |
//! |------|------|
//! | [`Cell`] | A grid position identified by `(row, column)`. |
//! | [`Values`] | A bitmap set of candidate values `1..=9` for a cell. |
//! | [`Cage`] | A polyomino paired with an [`Operation`]. |
//! | [`Puzzle`] | An `n×n` grid with a set of cages. |
//!
//! ## Entry points
//!
//! - **Generate** a random puzzle with [`generate::generate`] or [`generate::generate_with`]
//!   (custom operation policy / cage-size distribution).
//! - **Construct** a puzzle programmatically with [`Puzzle::new`] and [`Puzzle::insert_cage`].
//! - **Inspect** cell domains with [`Puzzle::get_cell_values`].

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

pub use arithmetic::Tuple;
pub use cage::{Cage, Operation, Operator};
pub use cell::{Cell, M, N, Values};
pub use error::Error;
pub use polyomino::Polyomino;
pub use puzzle::Puzzle;
