//! Cage representation pairing a polyomino with its arithmetic constraint.
//!
//! # Invariant: complete tuple support
//!
//! A `Cage` always stores the *complete* set of value tuples consistent with
//! both its arithmetic constraint and the current per-cell candidate fills.
//! Concretely: if a cell's candidate fill is `{a, b}`, then every tuple in
//! the backing memo has a value in `{a, b}` at that position, and every value
//! in `{a, b}` appears at that position in at least one tuple.
//!
//! This means [`Cage::set`] always recalculates from the full original tuple
//! set rather than narrowing incrementally. Widening a cell's fill therefore
//! restores tuples that were previously excluded, and narrowing it removes them.
//!
//! # Constraint kinds
//!
//! ## Commutative (add, multiply)
//!
//! Commutative operators are monotonically non-decreasing: extending a partial
//! tuple can only keep the accumulated result the same or increase it. This
//! monotonicity enables aggressive pruning during construction — branches whose
//! partial result already exceeds the target, or can no longer reach it, are cut
//! immediately. The result is stored as a [`Mdd`]: a DAG whose paths are exactly
//! the valid tuples, compressed by sharing common prefixes and suffixes.
//! Collinear distinctness (cells sharing a row or column must hold distinct values)
//! is encoded directly in the MDD's DP state during construction.
//!
//! ## Non-commutative (subtract, divide)
//!
//! Subtract and divide are not monotonic, so the MDD pruning strategy does not
//! apply. They are also inherently binary: Mathdoku defines subtract as
//! `|a − b|` and divide as `max(a, b) / min(a, b)`, neither of which generalises
//! meaningfully beyond a pair. Non-commutative cages are therefore always
//! dominoes (exactly 2 cells), and their constraint is stored as a [`Table`] —
//! the explicit list of valid pairs. Their operators (`|a−b| ≥ 1`, `max/min ≥ 2`)
//! already guarantee the two values differ, so no separate distinctness step is needed.
//!
//! ## Given
//!
//! A given cage is a singleton cell whose value is fixed by the puzzle author.
//! There is no arithmetic constraint and no memo: the value is stored directly.

use crate::csp::Constraint;
use crate::fill::Fill;
use crate::grid::Grid;
use crate::mdd::Mdd;
use crate::memo::Memo;
use crate::operator::{CommutativeOperator, NonCommutativeOperator};
use crate::polyomino::{Cell, Polyomino};
use crate::table::Table;
use crate::{Error, Error::EmptyFills, N, T};
use std::fmt::{Display, Formatter};

/// The arithmetic operation and target for a cage.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum CageOperator {
    /// Sum of all cell values equals the target.
    Add,
    /// Absolute difference of two cell values equals the target.
    Subtract,
    /// Product of all cell values equals the target.
    Multiply,
    /// Ratio `max/min` of two cell values equals the target.
    Divide,
    /// A single cell is fixed to the target value.
    Given,
}

/// The constraint for a cage and its supporting values.
#[derive(Clone, PartialEq, Eq, Debug)]
enum CageSupport {
    /// A commutative (monotonic) operation: add or multiply.
    Commutative(CommutativeOperator, T, Mdd),
    /// A non-commutative (non-monotonic) operation: subtract or divide.
    NonCommutative(NonCommutativeOperator, T, Table),
    /// A single cell with a fixed value.
    Given(N),
}

/// A cage: a connected group of cells subject to an arithmetic constraint.
///
/// The constraint is one of:
/// - **Commutative** (`Add`, `Multiply`): backed by an `Mdd` for efficient narrowing.
/// - **`NonCommutative`** (`Subtract`, `Divide`): backed by a `Table` of explicit pairs.
/// - **Given**: a singleton cell whose value is fixed.
#[derive(Debug, Clone)]
pub struct Cage {
    /// The cells belonging to this cage.
    pub polyomino: Polyomino,
    /// The constraint for this cage and its supporting values.
    support: CageSupport,
}

impl Cage {
    /// Constructs a cage from a [`CageOperator`], polyomino, and target value.
    ///
    /// # Panics
    ///
    /// Never panics in practice: the `Given` branch is only reached after confirming `k == 1`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InfeasibleCage`] if no tuples satisfy the constraint.
    /// Returns [`Error::MissingPolyomino`] if `operation` is [`CageOperator::Given`]
    /// and `polyomino` is empty.
    pub fn new(
        n: N,
        polyomino: Polyomino,
        operation: CageOperator,
        target: T,
    ) -> Result<Self, Error> {
        let k = polyomino.len();
        let result = match operation {
            CageOperator::Add => {
                Self::commutative(n, polyomino.clone(), CommutativeOperator::Add, target)
            }
            CageOperator::Multiply => {
                Self::commutative(n, polyomino.clone(), CommutativeOperator::Multiply, target)
            }
            CageOperator::Subtract => {
                if k != 2 {
                    return Err(Error::InfeasibleCage(polyomino, u64::from(target)));
                }
                Self::non_commutative(
                    n,
                    polyomino.clone(),
                    NonCommutativeOperator::Subtract,
                    target,
                )
            }
            CageOperator::Divide => {
                if k != 2 || target < 2 {
                    return Err(Error::InfeasibleCage(polyomino, u64::from(target)));
                }
                Self::non_commutative(n, polyomino.clone(), NonCommutativeOperator::Divide, target)
            }
            CageOperator::Given => {
                if k != 1 {
                    return Err(Error::InfeasibleCage(polyomino, u64::from(target)));
                }
                // k == 1 was just confirmed above, so `next()` always yields Some.
                let Some(&cell) = polyomino.iter().next() else {
                    return Err(Error::InfeasibleCage(polyomino, u64::from(target)));
                };
                let value = N::try_from(target)
                    .map_err(|_| Error::InfeasibleCage(polyomino, u64::from(target)))?;
                return Self::given(cell, value);
            }
        };
        result.map_err(|e| match e {
            EmptyFills => Error::InfeasibleCage(polyomino, u64::from(target)),
            other => other,
        })
    }

    /// Constructs a cage for a commutative constraint over `polyomino`.
    ///
    /// Builds an MDD representing all `polyomino.len()`-tuples of values in
    /// `1..=n` whose `operation` equals `target`, with collinear distinctness
    /// baked into the MDD's DP state.
    ///
    /// # Errors
    /// Returns [`EmptyFills`] if no tuples satisfy the constraint.
    pub fn commutative(
        n: N,
        polyomino: Polyomino,
        operation: CommutativeOperator,
        target: T,
    ) -> Result<Self, Error> {
        let k = N::try_from(polyomino.len()).map_err(|_| EmptyFills)?;
        let lines = collinear_groups(&polyomino);
        let mdd = Mdd::new(n, k, operation, target, &lines)?;
        let support = CageSupport::Commutative(operation, target, mdd);
        Ok(Self { polyomino, support })
    }

    /// Constructs a cage for a non-commutative constraint over `polyomino`.
    ///
    /// Builds a `Table` of all pairs of values in `1..=n` whose `operation`
    /// equals `target`. Non-commutative cages must be exactly 2 cells.
    /// Their operators (`|a−b| ≥ 1`, `max/min ≥ 2`) already guarantee distinct
    /// values, so no collinear distinctness step is needed.
    ///
    /// # Errors
    /// Returns [`EmptyFills`] if no pairs satisfy the constraint.
    pub fn non_commutative(
        n: N,
        polyomino: Polyomino,
        operation: NonCommutativeOperator,
        target: T,
    ) -> Result<Self, Error> {
        let table = Table::non_commutative(n, operation, target)?;
        let support = CageSupport::NonCommutative(operation, target, table);
        Ok(Self { polyomino, support })
    }

    /// Constructs a given cage: a single cell whose value is fixed to `target`.
    ///
    /// Always succeeds for a valid `cell`; returns `Err` only if the cell cannot
    /// form a polyomino, which cannot happen for a single non-empty cell.
    ///
    /// # Errors
    ///
    /// Returns an error if `polyomino` is empty.
    pub fn given(cell: Cell, n: N) -> Result<Self, Error> {
        Ok(Self {
            polyomino: Polyomino::from(vec![cell])?,
            support: CageSupport::Given(n),
        })
    }

    /// Returns the candidate [`Fill`] for `cell`.
    ///
    /// # Errors
    /// Returns [`Error::MissingCell`] if `cell` is not in a [`Cage`].
    pub fn get(&self, cell: Cell) -> Result<Fill, Error> {
        let index = self.polyomino_index(cell)?;
        let fill = match &self.support {
            CageSupport::Commutative(_, _, memo) => memo.get(index)?,
            CageSupport::NonCommutative(_, _, memo) => memo.get(index)?,
            CageSupport::Given(n) => Fill::from(&[*n]),
        };
        Ok(fill)
    }

    /// Returns the `(CageOperator, target)` pair for this cage.
    #[must_use]
    pub fn op_target(&self) -> (CageOperator, T) {
        match &self.support {
            CageSupport::Commutative(op, target, _) => (
                match op {
                    CommutativeOperator::Add => CageOperator::Add,
                    CommutativeOperator::Multiply => CageOperator::Multiply,
                },
                *target,
            ),
            CageSupport::NonCommutative(op, target, _) => (
                match op {
                    NonCommutativeOperator::Subtract => CageOperator::Subtract,
                    NonCommutativeOperator::Divide => CageOperator::Divide,
                },
                *target,
            ),
            CageSupport::Given(n) => (CageOperator::Given, T::from(*n)),
        }
    }

    /// Returns the index of `cell` in its containing [`Cage`].
    ///
    /// # Errors
    /// Returns [`Error::MissingCell`] if `cell` is not in a [`Cage`].
    fn polyomino_index(&self, cell: Cell) -> Result<usize, Error> {
        self.polyomino
            .iter()
            .position(|c| *c == cell)
            .ok_or(Error::MissingCell(cell))
    }

    /// Returns the polyomino (set of cells) for this cage.
    #[must_use]
    pub const fn polyomino(&self) -> &Polyomino {
        &self.polyomino
    }

    /// Returns the operation (operator and target) for this cage.
    #[must_use]
    pub fn operation(&self) -> Operation {
        let (operator, target) = self.op_target();
        Operation {
            operator,
            target: u64::from(target),
        }
    }

    /// Returns `true` if `cell` is part of this cage.
    #[must_use]
    pub fn contains(&self, cell: Cell) -> bool {
        self.polyomino.contains(&cell)
    }

    /// Returns the cells in this cage as a `Vec`.
    #[must_use]
    pub fn cells(&self) -> Vec<Cell> {
        self.polyomino.cells()
    }

    /// Returns the `(multiset, tuple)` counts of value assignments viable for
    /// this cage given the per-cell candidate `fills` (in polyomino order).
    ///
    /// The counts come from the cage's memo — the exact tuple relation —
    /// narrowed by `fills`, so they respect the arithmetic constraint,
    /// collinear distinctness, and the supplied fills jointly. For commutative
    /// cages the counts are folded over the narrowed `Mdd` in time
    /// proportional to the diagram, never by enumerating `n^k` combinations.
    ///
    /// # Errors
    /// Returns an error if the memo fails to narrow for a reason other than
    /// having no surviving tuples, which is reported as `(0, 0)`.
    pub fn viable_counts(&self, fills: &[Fill]) -> Result<(u64, u64), Error> {
        match &self.support {
            CageSupport::Given(value) => {
                let viable = fills.first().is_some_and(|fill| fill.contains(*value));
                Ok(if viable { (1, 1) } else { (0, 0) })
            }
            CageSupport::Commutative(_, _, memo) => match memo.narrow(fills) {
                Ok(narrowed) => Ok((narrowed.multiset_count(), narrowed.tuple_count())),
                Err(EmptyFills) => Ok((0, 0)),
                Err(e) => Err(e),
            },
            CageSupport::NonCommutative(_, _, memo) => match memo.narrow(fills) {
                Ok(narrowed) => {
                    let tuples = narrowed.tuples();
                    let multisets: std::collections::HashSet<Vec<N>> = tuples
                        .iter()
                        .map(|tuple| {
                            let mut sorted = tuple.clone();
                            sorted.sort_unstable();
                            sorted
                        })
                        .collect();
                    let count = |len: usize| u64::try_from(len).unwrap_or(u64::MAX);
                    Ok((count(multisets.len()), count(tuples.len())))
                }
                Err(EmptyFills) => Ok((0, 0)),
                Err(e) => Err(e),
            },
        }
    }
}

impl Display for Cage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cage({} {})",
            self.operation(),
            self.cells()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// The arithmetic operation for a cage: operator and target value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operation {
    /// The operator.
    pub operator: CageOperator,
    /// The target value.
    pub target: u64,
}

impl Operation {
    /// Creates an operation from `operator` and `target`.
    #[must_use]
    pub const fn new(operator: CageOperator, target: u64) -> Self {
        Self { operator, target }
    }
}

impl Display for CageOperator {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Add => write!(f, "+"),
            Self::Subtract => write!(f, "−"),
            Self::Multiply => write!(f, "×"),
            Self::Divide => write!(f, "÷"),
            Self::Given => write!(f, "="),
        }
    }
}

impl Display for Operation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.operator == CageOperator::Given {
            write!(f, "{}", self.target)
        } else {
            write!(f, "{}{}", self.operator, self.target)
        }
    }
}

/// Computes groups of cell indices (into the polyomino's iteration order) that share
/// a row or column and therefore must hold distinct values.
pub fn collinear_groups(polyomino: &Polyomino) -> Vec<Vec<usize>> {
    let cells: Vec<Cell> = polyomino.iter().copied().collect();
    let mut by_row: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    let mut by_col: std::collections::HashMap<usize, Vec<usize>> = std::collections::HashMap::new();
    for (i, &Cell(r, c)) in cells.iter().enumerate() {
        by_row.entry(r).or_default().push(i);
        by_col.entry(c).or_default().push(i);
    }
    by_row
        .into_values()
        .chain(by_col.into_values())
        .filter(|g| g.len() >= 2)
        .collect()
}

fn narrow_fills<M: Memo>(
    memo: &M,
    old_fills: &[Fill],
    n: usize,
    grid_n: usize,
) -> Result<Vec<Fill>, Error> {
    // All-full input fills exclude nothing, so narrowing would reproduce the
    // memo's cached base projection; return it directly and skip the narrow.
    let full = Fill::all(grid_n);
    if old_fills.iter().all(|&f| f == full) {
        return (0..n).map(|i| memo.get(i)).collect();
    }
    match memo.narrow(old_fills) {
        Ok(narrowed) => Ok((0..n)
            .map(|i| narrowed.get(i).unwrap_or_default())
            .collect()),
        Err(EmptyFills) => Ok(vec![Fill::default(); n]),
        Err(e) => Err(e),
    }
}

impl Constraint<Grid, Cell, Fill, Error> for Cage {
    fn propagate(&self, state: &Grid) -> Result<(Grid, Vec<Cell>), Error> {
        let cells: Vec<Cell> = self.polyomino.iter().copied().collect();
        let k = cells.len();
        let old_fills: Vec<Fill> = cells
            .iter()
            .map(|&c| state.get(c))
            .collect::<Result<_, _>>()?;
        let new_fills = match &self.support {
            CageSupport::Given(n) => {
                // Singleton cell: fill is always the fixed value, intersected with current state.
                let singleton = Fill::from(&[*n]);
                vec![if old_fills[0].contains(*n) {
                    singleton
                } else {
                    Fill::default()
                }]
            }
            // Commutative: collinear distinctness is encoded in the MDD — use narrow directly.
            CageSupport::Commutative(_, _, memo) => {
                narrow_fills(memo, &old_fills, k, state.size())?
            }
            // Non-commutative operators already guarantee distinct values (|a−b|≥1, max/min≥2).
            CageSupport::NonCommutative(_, _, memo) => {
                narrow_fills(memo, &old_fills, k, state.size())?
            }
        };
        Ok(state.apply_fills(&cells, &old_fills, new_fills))
    }

    fn in_scope(&self, variable: Cell) -> bool {
        self.polyomino.contains(&variable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::Grid;
    use crate::operator::CommutativeOperator::{Add, Multiply};
    use crate::operator::NonCommutativeOperator::{Divide, Subtract};

    fn domino(r0: usize, c0: usize, r1: usize, c1: usize) -> Polyomino {
        Polyomino::from([Cell(r0, c0), Cell(r1, c1)]).unwrap()
    }

    fn triomino(r0: usize, c0: usize, r1: usize, c1: usize, r2: usize, c2: usize) -> Polyomino {
        Polyomino::from([Cell(r0, c0), Cell(r1, c1), Cell(r2, c2)]).unwrap()
    }

    // ---- commutative ----

    #[test]
    fn commutative_add_succeeds() {
        assert!(Cage::commutative(4, domino(1, 1, 1, 2), Add, 5).is_ok());
    }

    #[test]
    fn commutative_multiply_succeeds() {
        assert!(Cage::commutative(4, domino(1, 1, 1, 2), Multiply, 6).is_ok());
    }

    #[test]
    fn commutative_triple_succeeds() {
        assert!(Cage::commutative(4, triomino(1, 1, 1, 2, 1, 3), Add, 6).is_ok());
    }

    #[test]
    fn commutative_infeasible_target_returns_empty_fills() {
        assert!(matches!(
            Cage::commutative(4, domino(1, 1, 1, 2), Add, 9),
            Err(EmptyFills)
        ));
    }

    #[test]
    fn commutative_stores_polyomino() {
        let poly = domino(1, 1, 1, 2);
        let cage = Cage::commutative(4, poly.clone(), Add, 5).unwrap();
        assert_eq!(cage.polyomino, poly);
    }

    // ---- non_commutative ----

    #[test]
    fn non_commutative_subtract_succeeds() {
        assert!(Cage::non_commutative(4, domino(1, 1, 1, 2), Subtract, 1).is_ok());
    }

    #[test]
    fn non_commutative_divide_succeeds() {
        assert!(Cage::non_commutative(4, domino(1, 1, 1, 2), Divide, 2).is_ok());
    }

    #[test]
    fn non_commutative_infeasible_target_returns_empty_fills() {
        // no pair in 1..=4 has |a-b| = 4
        assert!(matches!(
            Cage::non_commutative(4, domino(1, 1, 1, 2), Subtract, 4),
            Err(EmptyFills)
        ));
    }

    #[test]
    fn non_commutative_stores_polyomino() {
        let poly = domino(2, 1, 2, 2);
        let cage = Cage::non_commutative(4, poly.clone(), Subtract, 1).unwrap();
        assert_eq!(cage.polyomino, poly);
    }

    // ---- Constraint::propagate ----

    fn full_grid(n: usize) -> Grid {
        Grid::new(n).unwrap()
    }

    #[test]
    fn cage_propagate_given_pins_cell() {
        let cage = Cage::given(Cell(1, 1), 3).unwrap();
        let (new_g, changed) = cage.propagate(&full_grid(4)).unwrap();
        assert_eq!(new_g.get(Cell(1, 1)).unwrap(), Fill::from(&[3]));
        assert_eq!(changed, vec![Cell(1, 1)]);
    }

    #[test]
    fn cage_propagate_add_prunes_impossible_values() {
        // Add 3 in a 4×4: valid pairs summing to 3 are (1,2),(2,1) — only values {1,2}
        let cage = Cage::commutative(4, domino(1, 1, 1, 2), Add, 3).unwrap();
        let (new_g, _) = cage.propagate(&full_grid(4)).unwrap();
        assert_eq!(new_g.get(Cell(1, 1)).unwrap(), Fill::from(&[1, 2]));
        assert_eq!(new_g.get(Cell(1, 2)).unwrap(), Fill::from(&[1, 2]));
    }

    #[test]
    fn cage_propagate_cross_cell_add_prunes_partner() {
        // Add 5 in 4×4: valid pairs are (1,4),(2,3),(3,2),(4,1).
        // Pin cell A to {4}: only (4,1) survives, so B must narrow to {1}.
        let cage = Cage::commutative(4, domino(1, 1, 1, 2), Add, 5).unwrap();
        let g = full_grid(4).set(Cell(1, 1), Fill::from(&[4]));
        let (new_g, changed) = cage.propagate(&g).unwrap();
        assert_eq!(new_g.get(Cell(1, 2)).unwrap(), Fill::from(&[1]));
        assert!(changed.contains(&Cell(1, 2)));
    }

    #[test]
    fn cage_propagate_cross_cell_subtract_prunes_partner() {
        // Subtract 3 in 4×4: only valid pair is (4,1).
        // Pin cell A to {4}: B must narrow to {1}.
        let cage = Cage::non_commutative(4, domino(1, 1, 1, 2), Subtract, 3).unwrap();
        let g = full_grid(4).set(Cell(1, 1), Fill::from(&[4]));
        let (new_g, _) = cage.propagate(&g).unwrap();
        assert_eq!(new_g.get(Cell(1, 2)).unwrap(), Fill::from(&[1]));
    }

    #[test]
    fn cage_propagate_no_valid_tuple_empties_values() {
        // Grid has both cells pinned to {4}; Add 3 has no tuple (4,?) summing to 3
        let g = full_grid(4)
            .set(Cell(1, 1), Fill::from(&[4]))
            .set(Cell(1, 2), Fill::from(&[4]));
        let cage = Cage::commutative(4, domino(1, 1, 1, 2), Add, 3).unwrap();
        let (new_g, changed) = cage.propagate(&g).unwrap();
        assert!(new_g.get(Cell(1, 1)).unwrap().is_empty());
        assert!(new_g.get(Cell(1, 2)).unwrap().is_empty());
        assert_eq!(changed.len(), 2);
    }

    // ---- narrow_fills all-full short-circuit ----

    /// A memo wrapper whose `narrow` panics, proving the short-circuit path
    /// never reaches it.
    struct NoNarrow<M: Memo>(M);

    impl<M: Memo> Memo for NoNarrow<M> {
        fn get(&self, index: usize) -> Result<Fill, Error> {
            self.0.get(index)
        }
        fn narrow(&self, _support: &[Fill]) -> Result<Self, Error> {
            panic!("narrow must not be called when every input fill is full")
        }
    }

    #[test]
    fn narrow_fills_all_full_skips_narrow_and_returns_base_fills() {
        // Add 3 in a 4×4: base fills are {1,2} for both cells.
        let poly = domino(1, 1, 1, 2);
        let mdd = Mdd::new(4, 2, Add, 3, &collinear_groups(&poly)).unwrap();
        let full = vec![Fill::all(4); 2];
        let fills = narrow_fills(&NoNarrow(mdd), &full, 2, 4).unwrap();
        assert_eq!(fills, vec![Fill::from(&[1, 2]); 2]);
    }

    #[test]
    fn narrow_fills_all_full_matches_narrow_path() {
        let poly = triomino(1, 1, 1, 2, 2, 1);
        let mdd = Mdd::new(4, 3, Add, 7, &collinear_groups(&poly)).unwrap();
        let full = vec![Fill::all(4); 3];
        let shortcut = narrow_fills(&mdd, &full, 3, 4).unwrap();
        let narrowed = mdd.narrow(&full).unwrap();
        let via_narrow: Vec<Fill> = (0..3).map(|i| narrowed.get(i).unwrap()).collect();
        assert_eq!(shortcut, via_narrow);
    }

    #[test]
    fn narrow_fills_partial_input_still_narrows() {
        // Add 5 in 4×4 with cell A pinned to {4}: only (4,1) survives.
        let poly = domino(1, 1, 1, 2);
        let mdd = Mdd::new(4, 2, Add, 5, &collinear_groups(&poly)).unwrap();
        let fills = narrow_fills(&mdd, &[Fill::from(&[4]), Fill::all(4)], 2, 4).unwrap();
        assert_eq!(fills, vec![Fill::from(&[4]), Fill::from(&[1])]);
    }

    #[test]
    fn cage_propagate_subtract_full_grid_returns_base_fills() {
        // Subtract 3 in 4×4: valid pairs are (1,4),(4,1) — base fills {1,4}.
        // Exercises the short-circuit through the non-commutative (Table) arm.
        let cage = Cage::non_commutative(4, domino(1, 1, 1, 2), Subtract, 3).unwrap();
        let (new_g, _) = cage.propagate(&full_grid(4)).unwrap();
        assert_eq!(new_g.get(Cell(1, 1)).unwrap(), Fill::from(&[1, 4]));
        assert_eq!(new_g.get(Cell(1, 2)).unwrap(), Fill::from(&[1, 4]));
    }

    // ---- given ----

    #[test]
    fn given_succeeds() {
        assert!(Cage::given(Cell(1, 1), 3).is_ok());
    }

    #[test]
    fn given_stores_singleton_polyomino() {
        let cage = Cage::given(Cell(2, 3), 5).unwrap();
        assert!(cage.polyomino.contains(&Cell(2, 3)));
        assert_eq!(cage.polyomino.len(), 1);
    }

    #[test]
    fn given_stores_target_as_value() {
        let cage = Cage::given(Cell(1, 1), 7).unwrap();
        assert_eq!(cage.support, CageSupport::Given(7));
    }

    // ---- get ----

    #[test]
    fn get_missing_cell_returns_error() {
        let cage = Cage::commutative(4, domino(1, 1, 1, 2), Add, 5).unwrap();
        assert!(matches!(cage.get(Cell(9, 9)), Err(Error::MissingCell(_))));
    }

    #[test]
    fn get_given_returns_singleton_fill() {
        let cage = Cage::given(Cell(1, 1), 3).unwrap();
        assert_eq!(cage.get(Cell(1, 1)).unwrap(), Fill::from(&[3]));
    }
}
