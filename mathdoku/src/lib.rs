//! Mathdoku puzzle generator and solver.
//!
//! ## Core types
//!
//! | Type | Role |
//! |------|------|
//! | [`Cell`] | A grid position identified by `(row, column)`. |
//! | [`Values`] | A bitmap set of candidate values `1..=9` for a cell. |
//! | [`Cage`] | A polyomino paired with an [`Operation`]. |
//! | [`Puzzle`] | An `n×n` cage structure (no cell values). |
//! | [`Grid`] | An `n×n` grid of cell values. |
//! | [`Tuple`] | An ordered assignment of values to the cells of a cage. |
//!
//! ## Entry points
//!
//! - **Generate** a random puzzle with [`generate()`].
//! - **Construct** a puzzle programmatically with [`Puzzle::new`] and [`Puzzle::insert_cage`].
//! - **Inspect** cell values with [`Grid::cell_values`].
//! - **Solve** with [`Grid::solutions`].
//! - **Query valid operators** for a polyomino with [`operators`].
//!
//! ## Architecture
//!
//! Solving uses MAC (Maintaining Arc Consistency): [`Grid::solutions`] alternates between
//! branching on the most-constrained cell and propagating all constraints to a fixpoint.
//! Two propagators run on each fixpoint step:
//!
//! - **All-different** (rows and columns): Régin's GAC algorithm (internal `regin` module).
//! - **Cage arithmetic**: an MDD-based propagator computes per-cell GAC support in `O(|edges|)`
//!   using the MDD-4R algorithm (top-down reachability + bottom-up co-reachability sweep).

#![deny(missing_docs)]
// Test code leans on `.unwrap()`/`.expect()`/`panic!()` to assert invariants
// that the strict workspace policy denies in production. Allow them under
// `cfg(test)` so `cargo clippy --all-targets` stays green without scattering
// per-module `#[allow]`s. See issue #59.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod cage;
mod cell;
mod csp;
mod error;
mod generate;
mod grid;
mod grid_csp;
mod latin_square;
mod mdd;
mod operation;
mod polyomino;
mod puzzle;
mod regin;
#[cfg(test)]
mod test_utils;

pub use cage::Cage;
pub use cell::{Cell, Target, Tuple, Value, Values};
pub use error::Error;
pub use generate::generate;
pub use grid::Grid;
pub use latin_square::generate_latin_square;
pub use operation::{Operation, Operator, operators};
pub use polyomino::Polyomino;
pub use puzzle::Puzzle;
