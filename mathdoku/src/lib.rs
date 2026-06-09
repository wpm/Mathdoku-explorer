//! Mathdoku puzzle generator and solver.
//!
//! ## Core types
//!
//! | Type | Role |
//! |------|------|
//! | [`Cell`] | A grid position identified by `(row, column)`, 1-indexed. |
//! | [`Fill`] | A bitmap set of candidate values `1..=9` for a cell. |
//! | [`Polyomino`] | A connected set of cells forming a cage shape. |
//! | [`Puzzle`] | An `n×n` cage structure with constraint propagation. |
//! | [`CageOperator`] | The arithmetic operator for a cage (`Add`, `Subtract`, etc.). |
//!
//! ## Entry points
//!
//! - **Generate** a random puzzle with [`generate()`].
//! - **Construct** a puzzle programmatically with [`Puzzle::new`] and [`Puzzle::insert`].
//! - **Inspect** cell values with [`Puzzle::get`].
//! - **Solve** with [`Puzzle::solutions`].
//! - **Query valid operators** for a polyomino with [`operators_for`].

#![deny(missing_docs)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::print_stderr
    )
)]

pub(crate) mod cage;
pub(crate) mod csp;
pub(crate) mod fill;
mod generate;
pub(crate) mod grid;
mod latin_square;
pub(crate) mod mdd;
pub(crate) mod memo;
pub(crate) mod operator;
pub(crate) mod polyomino;
pub(crate) mod puzzle;
pub(crate) mod regin;
pub(crate) mod solutions;
pub(crate) mod table;
pub(crate) mod tuples;

pub use cage::{Cage, Operation};
pub use fill::Fill;
pub use generate::generate;
pub use latin_square::generate_latin_square;
pub use polyomino::{Cell, Polyomino};
pub use puzzle::{CageOperator, Puzzle, operators_for};

/// Initialises debug logging if the `MATHDOKU_DEBUG` environment variable is set to `1`.
///
/// Uses [`env_logger`] at `debug` level for all `mathdoku` targets. Safe to call
/// multiple times — subsequent calls after the first are no-ops.
pub fn init_debug_logging() {
    if std::env::var("MATHDOKU_DEBUG").as_deref() == Ok("1") {
        let _ = env_logger::builder()
            .filter_module("mathdoku", log::LevelFilter::Debug)
            .try_init();
    }
}

/// A [`Fill`] value or grid dimension in the range `1..=9`.
pub type N = u8;

/// The accumulated result of an arithmetic cage operation (sum or product of [`N`] values).
///
/// Sums and products of up to nine 9s can reach 729, which overflows `u8` and `u16`.
/// `u32` is wide enough for any realistic Mathdoku constraint.
pub type T = u32;

/// Alias for [`CageOperator`].
pub type Operator = CageOperator;

/// Alias for [`T`].
pub type Target = T;

/// Errors returned by mathdoku operations.
#[derive(Debug)]
pub enum Error {
    /// Invalid grid size.
    InvalidGridSize(usize),
    /// The cells do not form a connected polyomino.
    DisconnectedPolyomino,
    /// The [`Cell`] is missing from the specified polyomino or grid.
    MissingCell(Cell),
    /// The [`Polyomino`] contains cells not present in the puzzle grid.
    MissingPolyomino(Polyomino),
    /// Two polyominoes share one or more cells.
    CageConflict(Polyomino),
    /// No valid value assignment exists for the cage (operator/target infeasible).
    InfeasibleCage(Polyomino, u64),
    /// Invalid fill for a cage.
    InvalidCageFill(Polyomino, Fill),
    /// No candidate fills for a cage (internal solver state).
    EmptyFills,
    /// The index for a [`Cell`] in a cage is out of bounds.
    InvalidCellCageIndex(usize),
    /// Value not permitted in this [`Cell`].
    InvalidCellValue(Cell, N),
    /// A value passed to [`Fill::new`] is outside the valid range `1..=9`.
    InvalidValue(N),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidGridSize(n) => write!(f, "invalid grid size: {n}"),
            Self::DisconnectedPolyomino => write!(f, "cells do not form a connected polyomino"),
            Self::MissingCell(cell) => write!(f, "cell not in grid or polyomino: {cell}"),
            Self::InvalidCageFill(poly, fill) => {
                write!(f, "invalid fill {fill} for cage {poly:?}")
            }
            Self::EmptyFills => write!(f, "no candidate fills for cage"),
            Self::InvalidCellCageIndex(i) => write!(f, "cell cage index out of bounds: {i}"),
            Self::InvalidCellValue(cell, n) => {
                write!(f, "value {n} not a candidate for cell {cell}")
            }
            Self::MissingPolyomino(poly) => write!(f, "polyomino not in puzzle grid: {poly:?}"),
            Self::CageConflict(poly) => {
                write!(f, "cage overlaps existing cage: {poly:?}")
            }
            Self::InfeasibleCage(poly, target) => {
                write!(f, "no valid assignments for cage {poly:?} target {target}")
            }
            Self::InvalidValue(v) => write!(f, "value {v} is outside the valid range 1..=9"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fill::Fill;
    use crate::polyomino::{Cell, Polyomino};

    #[test]
    fn error_display_invalid_grid_size() {
        assert_eq!(
            Error::InvalidGridSize(0).to_string(),
            "invalid grid size: 0"
        );
    }

    #[test]
    fn error_display_missing_cell() {
        assert_eq!(
            Error::MissingCell(Cell(2, 3)).to_string(),
            "cell not in grid or polyomino: (2, 3)"
        );
    }

    #[test]
    fn error_display_empty_fills() {
        assert_eq!(Error::EmptyFills.to_string(), "no candidate fills for cage");
    }

    #[test]
    fn error_display_invalid_cell_value() {
        assert_eq!(
            Error::InvalidCellValue(Cell(1, 1), 5).to_string(),
            "value 5 not a candidate for cell (1, 1)"
        );
    }

    #[test]
    fn error_display_invalid_cell_cage_index() {
        assert_eq!(
            Error::InvalidCellCageIndex(3).to_string(),
            "cell cage index out of bounds: 3"
        );
    }

    #[test]
    fn error_display_disconnected_polyomino() {
        assert_eq!(
            Error::DisconnectedPolyomino.to_string(),
            "cells do not form a connected polyomino"
        );
    }

    #[test]
    fn error_display_invalid_cage_fill() {
        let poly = Polyomino::from([Cell(1, 1)]).unwrap();
        let fill = Fill::from(&[1, 2]);
        assert!(
            Error::InvalidCageFill(poly, fill)
                .to_string()
                .contains("invalid fill")
        );
    }
}
