//! A [`Cage`]: a polyomino with an arithmetic constraint.
//!
//! A cage combines a polyomino (the set of cells it covers) with an
//! [`Operation`] (an [`Operator`] and numeric target). [`Cage::tuples`] enumerates
//! every ordered assignment of values to the cage's cells that satisfies the
//! arithmetic constraint and the all-different rule within each shared row and
//! column of the polyomino.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::iter::{empty, once};

use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::arithmetic::{
    Tuple, addition_multisets, division_multisets, multiplication_multisets, subtraction_multisets,
};
use crate::polyomino::Polyomino;
use crate::{Cell, M, N};

/// A polyomino with an [`Operation`] constraining its cell values.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Cage {
    polyomino: Polyomino,
    operation: Operation,
}

impl Cage {
    #[must_use]
    pub const fn new(polyomino: Polyomino, operation: Operation) -> Self {
        Self {
            polyomino,
            operation,
        }
    }

    /// Returns the cells covered by this cage.
    #[must_use]
    pub fn cells(&self) -> Vec<Cell> {
        self.polyomino.cells()
    }

    /// Returns the operation (operator and target) for this cage.
    #[must_use]
    pub fn operation(&self) -> Operation {
        self.operation.clone()
    }

    /// Returns a reference to the polyomino for this cage.
    #[must_use]
    pub const fn polyomino(&self) -> &Polyomino {
        &self.polyomino
    }

    /// Returns all valid ordered value assignments for this cage in an `n`×`n` grid.
    ///
    /// Each tuple assigns one value from `1..=n` to each cell, in the row-major
    /// order of [`Cage::cells`]. Assignments that violate the all-different
    /// constraint within any shared row or column of the polyomino are excluded.
    #[allow(clippy::cast_possible_truncation)]
    pub fn tuples(&self, n: N) -> impl Iterator<Item = Tuple> {
        let k = self.polyomino.len();
        let target = self.operation.target;
        let multisets: Box<dyn Iterator<Item = Tuple>> = match self.operation.operator {
            Operator::Add => Box::new(addition_multisets(n, k, target as N)),
            Operator::Subtract => Box::new(subtraction_multisets(n, target as N)),
            Operator::Multiply => Box::new(multiplication_multisets(n, k, target)),
            Operator::Divide => Box::new(division_multisets(n, target as N)),
            Operator::Given => {
                if target >= 1 && target <= M::from(n) {
                    Box::new(once(vec![target as N]))
                } else {
                    Box::new(empty())
                }
            }
        };
        let filter = CollinearityFilter::new(&self.polyomino);
        multisets
            .flat_map(move |t| t.into_iter().permutations(k))
            .filter(move |t| filter.filter(t))
            .sorted()
            .dedup()
    }
}

impl Ord for Cage {
    fn cmp(&self, other: &Self) -> Ordering {
        self.polyomino.cmp(&other.polyomino)
    }
}

impl PartialOrd for Cage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// An [`Operator`] paired with a numeric target value imposed on a [`Cage`]'s cells.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Operation {
    pub operator: Operator,
    pub target: M,
}

impl Operation {
    #[must_use]
    pub const fn new(operator: Operator, target: M) -> Self {
        Self { operator, target }
    }
}

impl Display for Operation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.operator, self.target)
    }
}

/// The arithmetic operation a [`Cage`] imposes on its cells.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Operator {
    /// Cells sum to the target.
    Add,
    /// Two cells differ by the target.
    Subtract,
    /// Cells multiply to the target.
    Multiply,
    /// Two cells have a ratio equal to the target.
    Divide,
    /// A single cell is fixed to the target value.
    Given,
}

impl Display for Operator {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "×",
            Self::Divide => "÷",
            Self::Given => "",
        };
        write!(f, "{s}")
    }
}

/// Filters tuples that violate the all-different constraint within any row or
/// column of a cage's polyomino.
///
/// Precomputes the cell-index groups for each row and column once on
/// construction, then checks each candidate tuple against those groups.
struct CollinearityFilter {
    rows_and_columns: Vec<Vec<usize>>,
}

impl CollinearityFilter {
    /// Builds the filter for `polyomino`, grouping cell indices by shared row
    /// and column.
    fn new(polyomino: &Polyomino) -> Self {
        let cell_indexes: HashMap<Cell, usize> = polyomino
            .cells()
            .iter()
            .copied()
            .enumerate()
            .map(|(i, cell)| (cell, i))
            .collect();
        let to_indexes = |cells: Vec<Cell>| cells.iter().map(|cell| cell_indexes[cell]).collect();
        let rows = polyomino.rows().into_iter().map(&to_indexes);
        let columns = polyomino.columns().into_iter().map(&to_indexes);
        Self {
            rows_and_columns: rows.chain(columns).collect(),
        }
    }
    /// Returns `true` if `tuple` satisfies all-different within every row and
    /// column group of the polyomino.
    fn filter(&self, tuple: &Tuple) -> bool {
        self.rows_and_columns
            .iter()
            .all(|indexes| indexes.iter().map(|&i| tuple[i]).all_unique())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::test_utils::{col_pair, l_shape, pair, singleton};

    fn cage(polyomino: Polyomino, operator: Operator, target: M) -> Cage {
        Cage {
            polyomino,
            operation: Operation { operator, target },
        }
    }

    // --- Given ---

    #[test]
    fn given_singleton_yields_one_tuple() {
        let tuples: Vec<_> = cage(singleton(), Operator::Given, 3).tuples(4).collect();
        assert_eq!(tuples, vec![vec![3]]);
    }

    #[test]
    fn given_out_of_range_yields_no_tuples() {
        // target 5 is not in 1..=4
        assert!(
            cage(singleton(), Operator::Given, 5)
                .tuples(4)
                .next()
                .is_none()
        );
    }

    // --- Add ---

    #[test]
    fn add_pair_yields_correct_tuples() {
        // n=4, target=3: only {1,2} works; both orderings survive collinearity (same row)
        let tuples: Vec<_> = cage(pair(), Operator::Add, 3).tuples(4).collect();
        assert!(tuples.contains(&vec![1, 2]));
        assert!(tuples.contains(&vec![2, 1]));
        assert_eq!(tuples.len(), 2);
    }

    #[test]
    fn add_col_pair_collinearity_excludes_duplicates() {
        // col_pair: (0,0),(1,0) — the same column, so values must differ.
        // target=4, n=4: multisets are {1,3},{2,2},{3,1} but {2,2} has duplicate in column.
        let tuples: Vec<_> = cage(col_pair(), Operator::Add, 4).tuples(4).collect();
        for t in &tuples {
            assert_ne!(t[0], t[1], "collinear cells must not repeat a value");
        }
    }

    // --- Subtract ---

    #[test]
    fn subtract_pair_yields_correct_tuples() {
        // n=4, target=1: pairs differing by 1 are (1,2),(2,1),(2,3),(3,2),(3,4),(4,3)
        let tuples: Vec<_> = cage(pair(), Operator::Subtract, 1).tuples(4).collect();
        assert_eq!(tuples.len(), 6);
        for t in &tuples {
            let diff = (i32::from(t[0]) - i32::from(t[1])).unsigned_abs();
            assert_eq!(diff, 1);
        }
    }

    // --- Multiply ---

    #[test]
    fn multiply_pair_yields_correct_tuples() {
        // n=4, target=6: {2,3} → [2,3],[3,2]
        let tuples: Vec<_> = cage(pair(), Operator::Multiply, 6).tuples(4).collect();
        assert!(tuples.contains(&vec![2, 3]));
        assert!(tuples.contains(&vec![3, 2]));
        assert_eq!(tuples.len(), 2);
    }

    // --- Divide ---

    #[test]
    fn divide_pair_yields_correct_tuples() {
        // n=4, target=2: pairs with ratio 2 are (1,2),(2,1),(2,4),(4,2)
        let tuples: Vec<_> = cage(pair(), Operator::Divide, 2).tuples(4).collect();
        assert_eq!(tuples.len(), 4);
        for t in &tuples {
            let (a, b) = (u16::from(t[0]), u16::from(t[1]));
            assert!(a * 2 == b || b * 2 == a);
        }
    }

    // --- collinearity with l-shape ---

    #[test]
    fn l_shape_tuples_satisfy_row_and_column_all_different() {
        // l_shape: (0,0),(1,0),(1,1) — col 0 has cells 0 and 1; row 1 has cells 1 and 2.
        let tuples: Vec<_> = cage(l_shape(), Operator::Add, 6).tuples(4).collect();
        for t in &tuples {
            assert_ne!(t[0], t[1], "col 0: cells (0,0) and (1,0) must differ");
            assert_ne!(t[1], t[2], "row 1: cells (1,0) and (1,1) must differ");
        }
    }

    // --- Operator Display ---

    #[test]
    fn operator_display() {
        assert_eq!(Operator::Add.to_string(), "+");
        assert_eq!(Operator::Subtract.to_string(), "-");
        assert_eq!(Operator::Multiply.to_string(), "×");
        assert_eq!(Operator::Divide.to_string(), "÷");
        assert_eq!(Operator::Given.to_string(), "");
    }

    // --- Operation Display ---

    #[test]
    fn operation_display_with_symbol() {
        assert_eq!(Operation::new(Operator::Add, 12).to_string(), "+12");
        assert_eq!(Operation::new(Operator::Subtract, 3).to_string(), "-3");
        assert_eq!(Operation::new(Operator::Multiply, 24).to_string(), "×24");
        assert_eq!(Operation::new(Operator::Divide, 2).to_string(), "÷2");
    }

    #[test]
    fn operation_display_given_has_no_symbol() {
        assert_eq!(Operation::new(Operator::Given, 7).to_string(), "7");
    }
}
