//! The [`Grid`] type: an `n×n` grid of cell values.

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};

use crate::Error::InvalidGridSize;
use crate::cage::Cage;
use crate::puzzle::Puzzle;
use crate::{Cell, Error, Tuple, Value, Values};

// Serde wire format: flat struct with an n×n `values` array of cell value sets.
// `values` is optional on deserialization; absent means full value sets for all cells.
#[derive(Serialize, Deserialize)]
struct GridWire {
    n: usize,
    #[serde(default)]
    values: Vec<Vec<Values>>,
}

/// An `n×n` grid of cell values.
///
/// Each cell has a [`Values`] set — the candidate values `1..=n` still
/// consistent with the constraints applied so far. Use [`Grid::constrain`] to
/// propagate a [`Puzzle`]'s cage constraints into the grid.
///
/// `values` is a flat `[Values; 81]` array stored inline (no heap allocation).
/// Only the first `n*n` entries are used; the rest are `Values::default()`.
/// Cloning a `Grid` is a plain stack copy — no allocator involvement.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Grid {
    n: usize,
    values: [Values; 81],
}

impl Grid {
    /// Creates an `n×n` grid with all cell values initialized to `{1, ..., n}`.
    ///
    /// # Errors
    /// Returns [`InvalidGridSize`] if `n` is not in `1..=9`.
    pub fn new(n: usize) -> Result<Self, Error> {
        if !(1..=9).contains(&n) {
            return Err(InvalidGridSize(n));
        }
        let full = Values::all(n);
        let mut values = [Values::default(); 81];
        for slot in values.iter_mut().take(n * n) {
            *slot = full;
        }
        Ok(Self { n, values })
    }

    /// Returns the grid size `n` (grid is `n`×`n`).
    #[must_use]
    pub const fn n(&self) -> usize {
        self.n
    }

    /// Returns the current values of `cell`.
    ///
    /// # Errors
    /// Returns [`Error::InvalidCell`] if `cell` is outside the grid.
    pub fn cell_values(&self, cell: Cell) -> Result<Values, Error> {
        Ok(self.values[self.index(cell)?])
    }

    /// Returns a new grid with `cell`'s values narrowed to the singleton `{n}`.
    ///
    /// # Errors
    /// Returns [`Error::InvalidCell`] if `cell` is outside the grid.
    pub(crate) fn set_cell_value(&self, cell: Cell, n: Value) -> Result<Self, Error> {
        self.set_values(cell, Values::singleton(n))
    }

    /// Returns a new grid with `cell`'s values replaced by `values`.
    ///
    /// # Errors
    /// Returns [`Error::InvalidCell`] if `cell` is outside the grid.
    pub(crate) fn set_values(&self, cell: Cell, values: Values) -> Result<Self, Error> {
        let i = self.index(cell)?;
        let mut new_values = self.values;
        new_values[i] = values;
        Ok(Self {
            n: self.n,
            values: new_values,
        })
    }

    /// Creates a `Grid` whose cell values are the singleton values from `square`.
    ///
    /// `square` must be an `n×n` slice of rows, each row containing values in `1..=n`.
    ///
    /// # Errors
    /// Returns [`InvalidGridSize`] if `square.len() != n` or any row has length ≠ `n`,
    /// and [`Error::InvalidValue`] if any value is outside `1..=n`.
    pub fn from_latin_square(n: usize, square: &[Vec<Value>]) -> Result<Self, Error> {
        let mut grid = Self::new(n)?;
        for (r, row) in square.iter().enumerate() {
            for (c, &v) in row.iter().enumerate() {
                let cell = Cell::new(r, c);
                grid = grid.set_values(cell, Self::singleton_values(v))?;
            }
        }
        Ok(grid)
    }

    fn singleton_values(v: Value) -> Values {
        Values::singleton(v)
    }

    /// Propagates all constraints from `puzzle` to a fixpoint.
    ///
    /// Runs Régin's GAC on every row and column (all-different) and every cage,
    /// re-propagating any constraint adjacent to a cell whose values shrink, until
    /// no further pruning is possible.
    ///
    /// # Errors
    /// Returns [`InvalidGridSize`] if `puzzle.n() != self.n`, or an error
    /// if any cell is out of bounds during propagation.
    pub fn constrain(&self, puzzle: &Puzzle) -> Result<Self, Error> {
        if puzzle.n() != self.n {
            return Err(InvalidGridSize(puzzle.n()));
        }
        crate::grid_csp::grid_fixpoint(self, puzzle)
    }

    /// Returns a new grid with the values of `cells` reset to `{1..=n}` and
    /// all constraints re-propagated.
    ///
    /// This is the inverse of narrowing: use it when a constraint that was
    /// previously narrowing those cells is removed and their values may have
    /// widened beyond what the remaining constraints require.
    ///
    /// # Errors
    /// Returns [`InvalidGridSize`] if `puzzle.n() != self.n`, or an error
    /// if any cell is out of bounds or propagation fails.
    pub fn loosen(&self, cells: &[Cell], puzzle: &Puzzle) -> Result<Self, Error> {
        if puzzle.n() != self.n {
            return Err(InvalidGridSize(puzzle.n()));
        }
        let mut values = self.values;
        let full = Values::all(self.n);
        for &cell in cells {
            values[self.index(cell)?] = full;
        }
        Self { n: self.n, values }.constrain(puzzle)
    }

    /// Returns an iterator over all solutions for this grid under `puzzle`'s constraints.
    ///
    /// Each item is a solved [`Grid`] where every cell's values are a singleton.
    /// Uses MAC (Maintaining Arc Consistency): each branch is followed immediately by
    /// constraint propagation before the next branch is chosen.
    ///
    /// The iterator yields [`Err`] and stops if a propagation error occurs. Well-formed
    /// puzzle/grid pairs will never error.
    ///
    /// # Errors
    /// Returns [`InvalidGridSize`] if `puzzle.n() != self.n`.
    pub fn solutions<'a>(
        &'a self,
        puzzle: &'a Puzzle,
    ) -> impl Iterator<Item = Result<Self, Error>> + 'a {
        crate::grid_csp::Solutions::new(self, puzzle)
    }

    /// Returns `true` if every cell's values are a singleton.
    #[must_use]
    pub fn is_solution(&self) -> bool {
        (0..self.n)
            .flat_map(|r| (0..self.n).map(move |c| Cell::new(r, c)))
            .all(|cell| self.cell_values(cell).is_ok_and(Values::is_singleton))
    }

    /// Returns all valid ordered value assignments for `cage` given the current cell values.
    ///
    /// Each tuple assigns one value from `1..=n` to each cell in the cage, in
    /// the cage's cell order, filtered by the current values of each cell.
    /// Tuples are in lexicographic order.
    ///
    /// # Errors
    /// Returns [`Error::InvalidCage`] if `cage` is not present in `puzzle`.
    pub fn cage_tuples(&self, puzzle: &Puzzle, cage: &Cage) -> Result<Vec<Tuple>, Error> {
        if !puzzle.cages().any(|c| c == cage) {
            return Err(Error::InvalidCage(cage.clone()));
        }
        Ok(cage
            .mdd(self.n)
            .tuples()
            .filter(|tuple| {
                tuple
                    .iter()
                    .zip(cage.cells())
                    .all(|(&v, cell)| self.cell_values(cell).is_ok_and(|d| d.contains(v)))
            })
            .collect())
    }

    /// Returns a new grid with the cells of `cage` set to the values in the
    /// tuple at `index` (tuples in the same lexicographic order as [`cage_tuples`]),
    /// then propagated to a new constraint fixpoint.
    ///
    /// [`cage_tuples`]: Self::cage_tuples
    ///
    /// # Errors
    /// Returns [`Error::InvalidCage`] if `cage` is not present in `puzzle`, or
    /// [`Error::InvalidTupleIndex`] if `index` is out of range.
    pub fn set_cage_tuple(
        &self,
        puzzle: &Puzzle,
        cage: &Cage,
        index: usize,
    ) -> Result<Self, Error> {
        if !puzzle.cages().any(|c| c == cage) {
            return Err(Error::InvalidCage(cage.clone()));
        }
        let tuples: Vec<_> = cage.mdd(self.n).tuples().collect();
        let tuple = tuples
            .get(index)
            .ok_or(Error::InvalidTupleIndex(index, tuples.len()))?;
        let mut grid = self.clone();
        for (cell, &value) in cage.cells().iter().zip(tuple) {
            grid = grid.set_cell_value(*cell, value)?;
        }
        grid.constrain(puzzle)
    }

    pub(crate) const fn index(&self, cell: Cell) -> Result<usize, Error> {
        if cell.row < self.n && cell.column < self.n {
            Ok(cell.row * self.n + cell.column)
        } else {
            Err(Error::InvalidCell(cell))
        }
    }
}

impl Serialize for Grid {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let rows: Vec<Vec<Values>> = (0..self.n)
            .map(|r| (0..self.n).map(|c| self.values[r * self.n + c]).collect())
            .collect();
        GridWire {
            n: self.n,
            values: rows,
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for Grid {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let wire = GridWire::deserialize(d)?;
        let n = wire.n;
        if !(1..=9).contains(&n) {
            return Err(DeError::custom(format!("invalid grid size {n}")));
        }
        let mut values = [Values::default(); 81];
        if wire.values.is_empty() {
            let full = Values::all(n);
            for slot in values.iter_mut().take(n * n) {
                *slot = full;
            }
        } else {
            if wire.values.len() != n {
                return Err(DeError::custom(format!(
                    "expected {n} rows of values, got {}",
                    wire.values.len()
                )));
            }
            for (r, row) in wire.values.iter().enumerate() {
                if row.len() != n {
                    return Err(DeError::custom(format!(
                        "row {r}: expected {n} columns, got {}",
                        row.len()
                    )));
                }
            }
            for (slot, v) in values.iter_mut().zip(wire.values.into_iter().flatten()) {
                *slot = v;
            }
        }
        Ok(Self { n, values })
    }
}
impl Display for Grid {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}×{} grid", self.n, self.n)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, from_str, json, to_string};

    use super::*;
    use crate::Target;
    use crate::cage::Cage;
    use crate::operation::Operator::{Add, Divide, Given};
    use crate::operation::{Operation, Operator};
    use crate::polyomino::Polyomino;

    fn cage_at(positions: &[(usize, usize)], operator: Operator, target: Target) -> Cage {
        let cells: Vec<Cell> = positions.iter().map(|&(r, c)| Cell::new(r, c)).collect();
        let poly = Polyomino::from_cells(&cells).unwrap();
        Cage::new(poly, Operation::new(operator, target)).unwrap()
    }

    fn puzzle_with_cage(
        n: usize,
        positions: &[(usize, usize)],
        operator: Operator,
        target: Target,
    ) -> Puzzle {
        let cage = cage_at(positions, operator, target);
        Puzzle::new(n).unwrap().insert_cage(cage).unwrap()
    }

    // --- Grid::new ---

    #[test]
    fn new_valid_sizes_succeed() {
        for n in 1..=9 {
            assert!(Grid::new(n).is_ok(), "size {n} should succeed");
        }
    }

    #[test]
    fn new_size_zero_returns_err() {
        assert!(matches!(Grid::new(0), Err(InvalidGridSize(0))));
    }

    #[test]
    fn new_size_ten_returns_err() {
        assert!(matches!(Grid::new(10), Err(InvalidGridSize(10))));
    }

    #[test]
    fn new_values_are_full() {
        let g = Grid::new(4).unwrap();
        let expected = Values::all(4);
        for r in 0..4 {
            for c in 0..4 {
                assert_eq!(
                    g.cell_values(Cell::new(r, c)).unwrap(),
                    expected,
                    "cell ({r},{c}) should have full values"
                );
            }
        }
    }

    // --- Grid::cell_values ---

    #[test]
    fn get_cell_values_out_of_bounds_returns_err() {
        let g = Grid::new(3).unwrap();
        assert!(matches!(
            g.cell_values(Cell::new(3, 0)),
            Err(Error::InvalidCell(_))
        ));
        assert!(matches!(
            g.cell_values(Cell::new(0, 3)),
            Err(Error::InvalidCell(_))
        ));
    }

    // --- Grid::set_cell_value ---

    #[test]
    fn set_cell_values_narrows_values() {
        let g = Grid::new(4).unwrap();
        let cell = Cell::new(1, 2);
        let g2 = g.set_cell_value(cell, 3).unwrap();
        assert_eq!(g2.cell_values(cell).unwrap(), Values::new(&[3]).unwrap());
    }

    #[test]
    fn set_cell_values_is_non_destructive() {
        let g = Grid::new(4).unwrap();
        let cell = Cell::new(0, 0);
        let _ = g.set_cell_value(cell, 2).unwrap();
        // Original grid is unchanged.
        assert_eq!(g.cell_values(cell).unwrap(), Values::all(4));
    }

    #[test]
    fn set_cell_values_out_of_bounds_returns_err() {
        let g = Grid::new(3).unwrap();
        assert!(matches!(
            g.set_cell_value(Cell::new(3, 0), 1),
            Err(Error::InvalidCell(_))
        ));
    }

    // --- Grid::constrain ---

    // Builds a fully caged 2×2 puzzle and a Grid, verifies constrain pins every cell.
    //
    //   [1][2]
    //   [2][1]
    //
    fn solved_2x2_puzzle() -> Puzzle {
        Puzzle::new(2)
            .unwrap()
            .insert_cage(cage_at(&[(0, 0)], Given, 1))
            .unwrap()
            .insert_cage(cage_at(&[(0, 1)], Given, 2))
            .unwrap()
            .insert_cage(cage_at(&[(1, 0)], Given, 2))
            .unwrap()
            .insert_cage(cage_at(&[(1, 1)], Given, 1))
            .unwrap()
    }

    #[test]
    fn constrain_given_cages_pin_all_cells() {
        let puzzle = solved_2x2_puzzle();
        let g = Grid::new(2).unwrap().constrain(&puzzle).unwrap();
        assert_eq!(
            g.cell_values(Cell::new(0, 0)).unwrap(),
            Values::new(&[1]).unwrap()
        );
        assert_eq!(
            g.cell_values(Cell::new(0, 1)).unwrap(),
            Values::new(&[2]).unwrap()
        );
        assert_eq!(
            g.cell_values(Cell::new(1, 0)).unwrap(),
            Values::new(&[2]).unwrap()
        );
        assert_eq!(
            g.cell_values(Cell::new(1, 1)).unwrap(),
            Values::new(&[1]).unwrap()
        );
    }

    #[test]
    fn constrain_is_idempotent() {
        let puzzle = solved_2x2_puzzle();
        let g1 = Grid::new(2).unwrap().constrain(&puzzle).unwrap();
        let g2 = g1.constrain(&puzzle).unwrap();
        assert_eq!(g1, g2);
    }

    #[test]
    fn constrain_no_cages_unchanged() {
        let puzzle = Puzzle::new(2).unwrap();
        let g = Grid::new(2).unwrap();
        let g2 = g.constrain(&puzzle).unwrap();
        assert_eq!(g2, g);
    }

    #[test]
    fn constrain_size_mismatch_returns_err() {
        let puzzle = Puzzle::new(3).unwrap();
        let g = Grid::new(2).unwrap();
        assert!(matches!(g.constrain(&puzzle), Err(InvalidGridSize(_))));
    }

    // 2×2 with two arithmetic cages.
    #[test]
    fn constrain_arithmetic_cages_prune_2x2() {
        let puzzle = Puzzle::new(2)
            .unwrap()
            .insert_cage(cage_at(&[(0, 0), (0, 1)], Add, 3))
            .unwrap()
            .insert_cage(cage_at(&[(1, 0), (1, 1)], Divide, 2))
            .unwrap();
        let g = Grid::new(2).unwrap().constrain(&puzzle).unwrap();
        let expected = Values::new(&[1, 2]).unwrap();
        for r in 0..2 {
            for c in 0..2 {
                assert_eq!(
                    g.cell_values(Cell::new(r, c)).unwrap(),
                    expected,
                    "cell ({r},{c}) should be pruned to {{1,2}}"
                );
            }
        }
    }

    // --- Grid::solutions ---

    #[test]
    fn solutions_no_cages_yields_all_latin_squares() {
        let puzzle = Puzzle::new(2).unwrap();
        let g = Grid::new(2).unwrap();
        let solutions: Vec<Grid> = g.solutions(&puzzle).map(Result::unwrap).collect();
        assert_eq!(solutions.len(), 2);
        for sol in &solutions {
            for r in 0..2 {
                for c in 0..2 {
                    assert!(sol.cell_values(Cell::new(r, c)).unwrap().is_singleton());
                }
            }
        }
    }

    #[test]
    fn solutions_fully_caged_yields_one_solution() {
        let puzzle = solved_2x2_puzzle();
        let g = Grid::new(2).unwrap();
        let solutions: Vec<Grid> = g.solutions(&puzzle).map(Result::unwrap).collect();
        assert_eq!(solutions.len(), 1);
        let sol = &solutions[0];
        assert_eq!(
            sol.cell_values(Cell::new(0, 0)).unwrap(),
            Values::new(&[1]).unwrap()
        );
        assert_eq!(
            sol.cell_values(Cell::new(0, 1)).unwrap(),
            Values::new(&[2]).unwrap()
        );
        assert_eq!(
            sol.cell_values(Cell::new(1, 0)).unwrap(),
            Values::new(&[2]).unwrap()
        );
        assert_eq!(
            sol.cell_values(Cell::new(1, 1)).unwrap(),
            Values::new(&[1]).unwrap()
        );
    }

    #[test]
    fn solutions_infeasible_yields_none() {
        // Given cage with value 5 is out of range for a 2×2 (valid values: 1..=2).
        let puzzle = puzzle_with_cage(2, &[(0, 0)], Given, 5);
        let g = Grid::new(2).unwrap();
        assert!(g.solutions(&puzzle).map(Result::unwrap).next().is_none());
    }

    #[test]
    fn solutions_mixed_cages_unique_solution() {
        let puzzle = Puzzle::new(2)
            .unwrap()
            .insert_cage(cage_at(&[(0, 0), (0, 1)], Add, 3))
            .unwrap()
            .insert_cage(cage_at(&[(1, 0)], Given, 2))
            .unwrap()
            .insert_cage(cage_at(&[(1, 1)], Given, 1))
            .unwrap();
        let g = Grid::new(2).unwrap();
        let solutions: Vec<Grid> = g.solutions(&puzzle).map(Result::unwrap).collect();
        assert_eq!(solutions.len(), 1);
        let sol = &solutions[0];
        assert_eq!(
            sol.cell_values(Cell::new(0, 0)).unwrap(),
            Values::new(&[1]).unwrap()
        );
        assert_eq!(
            sol.cell_values(Cell::new(0, 1)).unwrap(),
            Values::new(&[2]).unwrap()
        );
        assert_eq!(
            sol.cell_values(Cell::new(1, 0)).unwrap(),
            Values::new(&[2]).unwrap()
        );
        assert_eq!(
            sol.cell_values(Cell::new(1, 1)).unwrap(),
            Values::new(&[1]).unwrap()
        );
    }

    #[test]
    fn solutions_3x3_row_sum_cages_all_valid() {
        let puzzle = Puzzle::new(3)
            .unwrap()
            .insert_cage(cage_at(&[(0, 0), (0, 1), (0, 2)], Add, 6))
            .unwrap()
            .insert_cage(cage_at(&[(1, 0), (1, 1), (1, 2)], Add, 6))
            .unwrap()
            .insert_cage(cage_at(&[(2, 0), (2, 1), (2, 2)], Add, 6))
            .unwrap();
        let g = Grid::new(3).unwrap();
        let solutions: Vec<Grid> = g.solutions(&puzzle).map(Result::unwrap).collect();
        assert!(!solutions.is_empty(), "should have at least one solution");
        for sol in &solutions {
            assert!(sol.is_solution());
            for r in 0..3 {
                let row_sum: u32 = (0..3)
                    .map(|c| u32::from(sol.cell_values(Cell::new(r, c)).unwrap().values()[0]))
                    .sum();
                assert_eq!(row_sum, 6, "row {r} should sum to 6");
            }
        }
    }

    #[test]
    fn solutions_4x4_mixed_cages_match_expected_set() {
        // End-to-end regression for the MDD cutover. A fully caged 4×4 puzzle
        // mixing cage shapes (givens, row pairs, a column pair, horizontal
        // triominoes) and operators (Given / Add / Subtract / Multiply). The
        // puzzle is intentionally under-determined: MDD-based cage propagation
        // must reproduce *exactly* the same three-solution set the old
        // multiset → permute → filter pipeline produced.
        let puzzle = Puzzle::new(4)
            .unwrap()
            .insert_cage(cage_at(&[(0, 0)], Given, 1))
            .unwrap()
            .insert_cage(cage_at(&[(0, 1), (0, 2)], Add, 5))
            .unwrap()
            .insert_cage(cage_at(&[(0, 3), (1, 3)], Add, 6))
            .unwrap()
            .insert_cage(cage_at(&[(1, 0), (1, 1)], Operator::Multiply, 12))
            .unwrap()
            .insert_cage(cage_at(&[(1, 2)], Given, 1))
            .unwrap()
            .insert_cage(cage_at(&[(2, 0), (3, 0)], Operator::Subtract, 2))
            .unwrap()
            .insert_cage(cage_at(&[(2, 1), (2, 2), (2, 3)], Operator::Multiply, 12))
            .unwrap()
            .insert_cage(cage_at(&[(3, 1), (3, 2), (3, 3)], Operator::Multiply, 6))
            .unwrap();

        let grid = Grid::new(4).unwrap();
        let mut actual: Vec<[[u8; 4]; 4]> = grid
            .solutions(&puzzle)
            .map(Result::unwrap)
            .map(|g| {
                let mut m = [[0u8; 4]; 4];
                for (r, row) in m.iter_mut().enumerate() {
                    for (c, slot) in row.iter_mut().enumerate() {
                        *slot = g.cell_values(Cell::new(r, c)).unwrap().values()[0];
                    }
                }
                m
            })
            .collect();
        actual.sort_unstable();

        // Independent of the expected set below, every returned grid must be a
        // genuine Latin square (each row and column a permutation of 1..=4).
        for m in &actual {
            for (i, row) in m.iter().enumerate() {
                let mut row = row.to_vec();
                let mut col: Vec<u8> = (0..4).map(|r| m[r][i]).collect();
                row.sort_unstable();
                col.sort_unstable();
                assert_eq!(row, vec![1, 2, 3, 4], "row {i} is not a permutation");
                assert_eq!(col, vec![1, 2, 3, 4], "column {i} is not a permutation");
            }
        }

        let mut expected = [
            [[1, 3, 2, 4], [3, 4, 1, 2], [2, 1, 4, 3], [4, 2, 3, 1]],
            [[1, 2, 3, 4], [3, 4, 1, 2], [2, 3, 4, 1], [4, 1, 2, 3]],
            [[1, 2, 3, 4], [3, 4, 1, 2], [2, 1, 4, 3], [4, 3, 2, 1]],
        ];
        expected.sort_unstable();
        assert_eq!(actual, expected);
    }

    // --- Grid::set_cage_tuple ---

    #[test]
    fn set_cage_tuple_pins_cells_in_lexicographic_order() {
        // Add cage over (0,0),(0,1) with target 3: the lexicographically first
        // valid tuple is [1, 2], so index 0 pins (0,0)=1 and (0,1)=2.
        let puzzle = puzzle_with_cage(4, &[(0, 0), (0, 1)], Add, 3);
        let cage = puzzle.cages().next().unwrap().clone();
        let grid = Grid::new(4).unwrap();
        let set = grid.set_cage_tuple(&puzzle, &cage, 0).unwrap();
        assert_eq!(
            set.cell_values(Cell::new(0, 0)).unwrap(),
            Values::new(&[1]).unwrap()
        );
        assert_eq!(
            set.cell_values(Cell::new(0, 1)).unwrap(),
            Values::new(&[2]).unwrap()
        );
    }

    #[test]
    fn set_cage_tuple_out_of_range_index_errors() {
        let puzzle = puzzle_with_cage(4, &[(0, 0), (0, 1)], Add, 3);
        let cage = puzzle.cages().next().unwrap().clone();
        let grid = Grid::new(4).unwrap();
        assert!(matches!(
            grid.set_cage_tuple(&puzzle, &cage, 999),
            Err(Error::InvalidTupleIndex(999, _))
        ));
    }

    // --- serde round-trip ---

    #[test]
    fn grid_round_trips_through_json() {
        let g = Grid::new(3)
            .unwrap()
            .set_cell_value(Cell::new(0, 0), 2)
            .unwrap();
        let json = to_string(&g).unwrap();
        let restored: Grid = from_str(&json).unwrap();
        assert_eq!(g, restored);
    }

    #[test]
    fn grid_deserialize_invalid_n_returns_err() {
        let json = r#"{"n":0,"values":[]}"#;
        assert!(from_str::<Grid>(json).is_err());
        let json = r#"{"n":10,"values":[]}"#;
        assert!(from_str::<Grid>(json).is_err());
    }

    #[test]
    fn grid_deserialize_wrong_row_count_returns_err() {
        let json = r#"{"n":2,"values":[[1,2]]}"#;
        assert!(from_str::<Grid>(json).is_err());
    }

    #[test]
    fn grid_deserialize_wrong_column_count_returns_err() {
        let json = r#"{"n":2,"values":[[1,2,3],[1,2,3]]}"#;
        assert!(from_str::<Grid>(json).is_err());
    }

    #[test]
    fn grid_serialize_values_are_row_major() {
        let g = Grid::new(2)
            .unwrap()
            .set_cell_value(Cell::new(0, 0), 1)
            .unwrap();
        let json = to_string(&g).unwrap();
        let v: Value = from_str(&json).unwrap();
        // values[0][0] should be the singleton [1]
        assert_eq!(v["values"][0][0], json!([1]));
    }

    #[test]
    fn grid_deserialize_absent_values_uses_full_value_sets() {
        let json = r#"{"n":3}"#;
        let g: Grid = from_str(json).unwrap();
        assert_eq!(g.n(), 3);
        for r in 0..3 {
            for c in 0..3 {
                assert_eq!(g.cell_values(Cell::new(r, c)).unwrap(), Values::all(3));
            }
        }
    }

    // --- Grid::cage_tuples ---

    #[test]
    fn cage_tuples_returns_valid_tuples() {
        let cage = cage_at(&[(0, 0), (0, 1)], Add, 3);
        let puzzle = Puzzle::new(4).unwrap().insert_cage(cage.clone()).unwrap();
        let g = Grid::new(4).unwrap();
        let tuples = g.cage_tuples(&puzzle, &cage).unwrap();
        assert!(!tuples.is_empty());
        for t in &tuples {
            let sum: Target = t.iter().map(|&v| Target::from(v)).sum();
            assert_eq!(sum, 3);
        }
    }

    #[test]
    fn display_shows_dimensions() {
        assert_eq!(Grid::new(4).unwrap().to_string(), "4×4 grid");
    }

    #[test]
    fn cage_tuples_invalid_cage_returns_err() {
        let puzzle = Puzzle::new(4).unwrap();
        let cage = cage_at(&[(0, 0)], Given, 1);
        let g = Grid::new(4).unwrap();
        assert!(matches!(
            g.cage_tuples(&puzzle, &cage),
            Err(Error::InvalidCage(_))
        ));
    }
}
