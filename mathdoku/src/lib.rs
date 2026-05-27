//! Mathdoku puzzle generator and solver.
//!
//! ## Core types
//!
//! | Type | Role |
//! |------|------|
//! | [`Cell`] | A grid position identified by `(row, column)`. |
//! | [`Values`] | A bitmap set of candidate values `1..=9` for a cell. |
//! | [`Cage`] | A polyomino paired with an [`Operation`]. |
//! | [`Puzzle`] | An `n×n` cage structure (no cell domains). |
//! | [`Grid`] | An `n×n` grid of cell domains. |
//! | [`Tuple`] | An ordered assignment of values to the cells of a cage. |
//! | [`Mdd`] | A reduced ordered MDD over a cage's valid tuples. |
//!
//! ## Entry points
//!
//! - **Generate** a random puzzle with [`generate()`] or [`generate::generate_with`] (custom
//!   operation policy / cage-size distribution).
//! - **Construct** a puzzle programmatically with [`Puzzle::new`] and [`Puzzle::insert_cage`].
//! - **Inspect** cell domains with [`Grid::cell_values`].
//! - **Solve** with [`Grid::solutions`].
//! - **Enumerate a cage's valid assignments** with [`Cage::mdd`] then [`Mdd::tuples`].
//! - **Query valid operators** for a polyomino with [`operators`].
//!
//! ## Architecture
//!
//! Solving uses MAC (Maintaining Arc Consistency): [`Grid::solutions`] alternates between
//! branching on the most-constrained cell and propagating all constraints to a fixpoint.
//! Two propagators run on each fixpoint step:
//!
//! - **All-different** (rows and columns): Régin's GAC algorithm (internal `regin` module).
//! - **Cage arithmetic**: [`Mdd::support`] computes per-cell GAC support in `O(|edges|)`
//!   using the MDD-4R algorithm (top-down reachability + bottom-up co-reachability sweep).
//!   See [`mdd`] for the full algorithm and references.

mod cage;
mod cell;
mod csp;
mod error;
pub mod generate;
pub mod grid;
mod grid_csp;
pub mod latin_square;
pub mod mdd;
pub mod operation;
mod polyomino;
pub mod puzzle;
mod regin;
#[cfg(test)]
mod test_utils;

pub use cage::Cage;
pub use cell::{Cell, M, N, Tuple, Values};
pub use error::Error;
pub use generate::generate;
pub use grid::Grid;
pub use latin_square::generate_latin_square;
pub use mdd::Mdd;
pub use operation::{Operation, Operator, operators};
pub use polyomino::Polyomino;
pub use puzzle::Puzzle;
