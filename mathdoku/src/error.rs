//! The [`Error`] type returned by fallible puzzle and polyomino operations.
//!
//! Every variant carries enough context to produce a human-readable message via
//! [`fmt::Display`], and the enum implements [`std::error::Error`] so it
//! composes with standard error-handling idioms.

use std::error::Error as StdError;
use std::fmt;

use crate::cage::Cage;
use crate::cell::Cell;
use crate::operation::{Operation, Operator};
use crate::polyomino::Polyomino;
use crate::{Grid, Puzzle};

/// Errors that can occur during puzzle construction or solving.
#[derive(Debug)]
pub enum Error {
    /// A puzzle was constructed with a size less than 1 or greater than 9.
    InvalidGridSize(usize),
    /// A referenced [`Cell`] is not present in the grid.
    InvalidCell(Cell),
    /// A new [`Cage`] conflicts with an existing cage.
    CageConflict(Cage),
    /// A polyomino cannot support the requested [`Operation`]: either the
    /// operator is invalid for the cell count, or the target is unreachable.
    InfeasibleOperation(Polyomino, Operation),
    /// The arity of a tuple does not match the [`Operator`]'s requirements.
    InvalidOperationArity(Operator, usize),
    /// A [`Cell`] referenced by an operation is not covered by any polyomino.
    CellNotCovered(Cell),
    /// Removing a [`Cell`] from a polyomino would disconnect the remaining cells.
    WouldDisconnect(Cell),
    /// A [`Cell`] is not edge-adjacent to the polyomino it was applied to.
    TargetNotAdjacent,
    /// A [`Cell`] is already present in the polyomino.
    CellAlreadyInPolyomino(Cell),
    /// Removing a [`Cell`] would leave the polyomino empty.
    RemovalWouldEmptyPolyomino(Cell),
    /// A polyomino was constructed from an empty cell slice.
    EmptyPolyomino,
    /// A polyomino was constructed from cells that are not edge-connected.
    DisconnectedPolyomino,
    /// A grid index is out of range for the given grid size. Carries `(index, n)`.
    IndexOutOfRange(usize, usize),
    /// An operation policy received an empty value slice.
    EmptyOpPolicyValues,
    /// A fold operation was applied to an empty tuple.
    EmptyTuple,
    /// The referenced [`Cage`] is not present in the puzzle.
    InvalidCage(Cage),
    /// A tuple index is out of range for the cage. Carries `(index, len)`.
    InvalidTupleIndex(usize, usize),
    /// A value passed to `Values::new` is outside the valid range `1..=9`.
    InvalidValue(crate::cell::Value),
    /// The grid and puzzle dimensions do not match.
    GridPuzzleMismatch(Box<Grid>, Box<Puzzle>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGridSize(n) => write!(f, "invalid grid size {n}"),
            Self::InvalidCell(c) => {
                write!(f, "cell ({}, {}) is outside the grid", c.row, c.column)
            }
            Self::CageConflict(new) => {
                write!(
                    f,
                    "cage {new:?} conflicts with an existing cage in the puzzle"
                )
            }
            Self::InfeasibleOperation(p, op) => {
                write!(f, "operation {op:?} is infeasible for polyomino {p:?}")
            }
            Self::CellNotCovered(c) => write!(
                f,
                "cell ({}, {}) is not covered by any polyomino",
                c.row, c.column
            ),
            Self::WouldDisconnect(c) => write!(
                f,
                "removing cell ({}, {}) would disconnect the polyomino",
                c.row, c.column
            ),
            Self::TargetNotAdjacent => {
                write!(f, "target cell is not edge-adjacent to the polyomino")
            }
            Self::CellAlreadyInPolyomino(c) => write!(
                f,
                "cell ({}, {}) is already in the polyomino",
                c.row, c.column
            ),
            Self::RemovalWouldEmptyPolyomino(c) => write!(
                f,
                "removing cell ({}, {}) would leave an empty polyomino",
                c.row, c.column
            ),
            Self::EmptyPolyomino => write!(
                f,
                "polyomino cannot be constructed from an empty cell slice"
            ),
            Self::DisconnectedPolyomino => write!(f, "polyomino cells are not edge-connected"),
            Self::IndexOutOfRange(index, n) => {
                write!(f, "index {index} is out of range for grid of size {n}")
            }
            Self::EmptyOpPolicyValues => {
                write!(f, "operation policy received an empty value slice")
            }
            Self::EmptyTuple => {
                write!(f, "tuple operation cannot be applied to an empty tuple")
            }
            Self::InvalidCage(cage) => write!(f, "cage {cage:?} is not present in the puzzle"),
            Self::InvalidTupleIndex(index, len) => {
                write!(
                    f,
                    "tuple index {index} is out of range for cage with {len} tuples"
                )
            }
            Self::InvalidOperationArity(operator, arity) => write!(
                f,
                "{operator} cannot be applied to a tuple of arity {arity}"
            ),
            Self::InvalidValue(v) => write!(f, "value {v} is outside the valid range 1..=9"),
            Self::GridPuzzleMismatch(grid, puzzle) => {
                write!(f, "{grid} and {puzzle} are different sizes")
            }
        }
    }
}

impl StdError for Error {}

#[cfg(test)]
mod tests {
    use crate::cage::Cage;
    use crate::operation::{Operation, Operator};
    use crate::polyomino::Polyomino;
    use crate::{Cell, Error};
    #[test]
    fn error_display_covers_all_variants() {
        let c = Cell::new(1, 2);
        assert_eq!(Error::InvalidGridSize(0).to_string(), "invalid grid size 0");
        assert_eq!(
            Error::InvalidCell(c).to_string(),
            "cell (1, 2) is outside the grid"
        );
        assert_eq!(
            Error::CellNotCovered(c).to_string(),
            "cell (1, 2) is not covered by any polyomino"
        );
        assert_eq!(
            Error::WouldDisconnect(c).to_string(),
            "removing cell (1, 2) would disconnect the polyomino"
        );
        assert_eq!(
            Error::TargetNotAdjacent.to_string(),
            "target cell is not edge-adjacent to the polyomino"
        );
        assert_eq!(
            Error::CellAlreadyInPolyomino(c).to_string(),
            "cell (1, 2) is already in the polyomino"
        );
        assert_eq!(
            Error::RemovalWouldEmptyPolyomino(c).to_string(),
            "removing cell (1, 2) would leave an empty polyomino"
        );
        assert_eq!(
            Error::EmptyPolyomino.to_string(),
            "polyomino cannot be constructed from an empty cell slice"
        );
        assert_eq!(
            Error::DisconnectedPolyomino.to_string(),
            "polyomino cells are not edge-connected"
        );
        assert_eq!(
            Error::IndexOutOfRange(3, 2).to_string(),
            "index 3 is out of range for grid of size 2"
        );
        assert_eq!(
            Error::EmptyOpPolicyValues.to_string(),
            "operation policy received an empty value slice"
        );
        assert_eq!(
            Error::EmptyTuple.to_string(),
            "tuple operation cannot be applied to an empty tuple"
        );
        let poly = Polyomino::from_cells(&[Cell::new(0, 0)]).unwrap();
        let cage = Cage::new(poly, Operation::new(Operator::Given, 1)).unwrap();
        assert!(
            Error::InvalidCage(cage)
                .to_string()
                .contains("not present in the puzzle")
        );
        assert_eq!(
            Error::InvalidTupleIndex(3, 2).to_string(),
            "tuple index 3 is out of range for cage with 2 tuples"
        );
        assert_eq!(
            Error::InvalidValue(10).to_string(),
            "value 10 is outside the valid range 1..=9"
        );
    }
}
