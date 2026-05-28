//! The cage structure and constrained cell values for the puzzle being designed,
//! shared via context for on-demand queries.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use mathdoku::{Cage, Cell, Grid, Puzzle};

/// The cage structure and constrained cell values for the puzzle being designed,
/// shared via context for on-demand queries.
///
/// The `Mutex` is needed only to satisfy `Send + Sync` for `provide_context`; on
/// single-threaded WASM there is never actual contention.
#[derive(Clone)]
pub struct PartialSolution(Arc<PartialSolutionInner>);

struct PartialSolutionInner {
    puzzle: Mutex<Puzzle>,
    grid: Mutex<Grid>,
}

impl PartialSolution {
    #[must_use]
    pub fn new(puzzle: Puzzle, grid: Grid) -> Self {
        Self(Arc::new(PartialSolutionInner {
            puzzle: Mutex::new(puzzle),
            grid: Mutex::new(grid),
        }))
    }

    fn lock_puzzle(&self) -> std::sync::MutexGuard<'_, Puzzle> {
        self.0.puzzle.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_grid(&self) -> std::sync::MutexGuard<'_, Grid> {
        self.0.grid.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Returns the number of solutions, or `None` if not all cells are covered by cages.
    ///
    /// Counts are found by propagating all cage constraints forward from an
    /// unconstrained grid, so the result reflects the puzzle's actual solution
    /// space rather than a specific Latin square.
    pub fn solution_count(&self) -> Option<usize> {
        let puzzle = self.lock_puzzle().clone();
        let n = puzzle.n();
        let covered: HashSet<_> = puzzle.cages().flat_map(Cage::cells).collect();
        if covered.len() < n * n {
            return None;
        }
        let propagated = Grid::new(n).ok()?.constrain(&puzzle).ok()?;
        Some(propagated.solutions(&puzzle).count())
    }

    /// Returns the singleton solution value for `cell`, or `None` if not a singleton.
    #[must_use]
    pub fn cell_value_singleton(&self, cell: Cell) -> Option<u8> {
        let grid = self.lock_grid();
        let v = grid.cell_values(cell).ok()?;
        drop(grid);
        v.is_singleton().then(|| v.values().first().copied())?
    }

    /// Returns `(multisets, tuples)` for `cage_idx`, or `None` if out of range.
    ///
    /// Counts are computed by propagating all cage constraints forward from an
    /// unconstrained grid, so they reflect every cage currently on the puzzle.
    #[must_use]
    pub fn viable_counts(&self, cage_idx: usize) -> Option<(usize, usize)> {
        let puzzle = self.lock_puzzle();
        let cage = puzzle.cages().nth(cage_idx)?;
        let n = puzzle.n();
        // Propagate all cage constraints from a fresh unconstrained grid.
        let propagated = Grid::new(n).ok()?.constrain(&puzzle).ok()?;
        let tuples = propagated.cage_tuples(&puzzle, cage).ok()?;
        drop(puzzle);
        let multisets: HashSet<Vec<u8>> = tuples
            .iter()
            .map(|t| {
                let mut s = t.clone();
                s.sort_unstable();
                s
            })
            .collect();
        Some((multisets.len(), tuples.len()))
    }

    /// Returns the cage index for the cell at `(r, c)`, or `None` if uncovered.
    #[must_use]
    pub fn cage_index_at(&self, r: usize, c: usize) -> Option<usize> {
        let cell = Cell::new(r, c);
        let puzzle = self.lock_puzzle();
        puzzle
            .cages()
            .enumerate()
            .find(|(_, cage)| cage.cells().contains(&cell))
            .map(|(i, _)| i)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::PartialSolution;
    use mathdoku::{Cage, Cell, Grid, Operation, Operator, Polyomino, Puzzle};

    fn cage_at(positions: &[(usize, usize)], op: Operator, target: u64) -> Cage {
        let cells: Vec<Cell> = positions.iter().map(|&(r, c)| Cell::new(r, c)).collect();
        let poly = Polyomino::from_cells(&cells).unwrap();
        Cage::new(poly, Operation::new(op, target))
    }

    /// A 3×3 puzzle whose cells are pinned to the Latin square
    /// ```text
    /// 1 2 3
    /// 2 3 1
    /// 3 1 2
    /// ```
    /// by nine `Given` cages — exactly one solution.
    fn given_3x3() -> Puzzle {
        let square = [[1u64, 2, 3], [2, 3, 1], [3, 1, 2]];
        let mut puzzle = Puzzle::new(3).unwrap();
        for (r, row) in square.iter().enumerate() {
            for (c, &v) in row.iter().enumerate() {
                puzzle = puzzle
                    .insert_cage(cage_at(&[(r, c)], Operator::Given, v))
                    .unwrap();
            }
        }
        puzzle
    }

    /// A 3×3 puzzle covered by three `Add`-6 row cages. Every row is forced to be
    /// a permutation of `{1,2,3}`, so the solutions are exactly the 12 order-3
    /// Latin squares.
    fn row_sums_3x3() -> Puzzle {
        let mut puzzle = Puzzle::new(3).unwrap();
        for r in 0..3 {
            puzzle = puzzle
                .insert_cage(cage_at(&[(r, 0), (r, 1), (r, 2)], Operator::Add, 6))
                .unwrap();
        }
        puzzle
    }

    #[test]
    fn solution_count_unique_puzzle_is_one() {
        let puzzle = given_3x3();
        let ps = PartialSolution::new(puzzle, Grid::new(3).unwrap());
        assert_eq!(ps.solution_count(), Some(1));
    }

    #[test]
    fn solution_count_row_sums_counts_all_latin_squares() {
        let puzzle = row_sums_3x3();
        let ps = PartialSolution::new(puzzle, Grid::new(3).unwrap());
        assert_eq!(ps.solution_count(), Some(12));
    }

    #[test]
    fn solution_count_incomplete_coverage_is_none() {
        // Only one cell is covered, so most of the grid is uncaged.
        let puzzle = Puzzle::new(3)
            .unwrap()
            .insert_cage(cage_at(&[(0, 0)], Operator::Given, 1))
            .unwrap();
        let ps = PartialSolution::new(puzzle, Grid::new(3).unwrap());
        assert_eq!(ps.solution_count(), None);
    }

    #[test]
    fn cell_value_singleton_reads_pinned_grid() {
        let square = vec![vec![1u8, 2, 3], vec![2, 3, 1], vec![3, 1, 2]];
        let grid = Grid::from_latin_square(3, &square).unwrap();
        let ps = PartialSolution::new(Puzzle::new(3).unwrap(), grid);
        assert_eq!(ps.cell_value_singleton(Cell::new(0, 0)), Some(1));
        assert_eq!(ps.cell_value_singleton(Cell::new(1, 0)), Some(2));
        assert_eq!(ps.cell_value_singleton(Cell::new(2, 2)), Some(2));
    }

    #[test]
    fn cell_value_singleton_none_when_values_not_singleton() {
        // A fresh grid has the full values {1,2,3} in every cell.
        let ps = PartialSolution::new(Puzzle::new(3).unwrap(), Grid::new(3).unwrap());
        assert_eq!(ps.cell_value_singleton(Cell::new(0, 0)), None);
    }

    #[test]
    fn viable_counts_row_cage_has_one_multiset_six_tuples() {
        let puzzle = row_sums_3x3();
        let ps = PartialSolution::new(puzzle, Grid::new(3).unwrap());
        // The only multiset summing to 6 with distinct values in 1..=3 is {1,2,3},
        // which has 3! = 6 ordered arrangements.
        assert_eq!(ps.viable_counts(0), Some((1, 6)));
    }

    #[test]
    fn viable_counts_out_of_range_is_none() {
        let puzzle = row_sums_3x3();
        let ps = PartialSolution::new(puzzle, Grid::new(3).unwrap());
        assert_eq!(ps.viable_counts(99), None);
    }

    #[test]
    fn cage_index_at_covered_and_uncovered() {
        let puzzle = Puzzle::new(3)
            .unwrap()
            .insert_cage(cage_at(&[(0, 0), (0, 1)], Operator::Add, 3))
            .unwrap();
        let ps = PartialSolution::new(puzzle, Grid::new(3).unwrap());
        assert_eq!(ps.cage_index_at(0, 0), Some(0));
        assert_eq!(ps.cage_index_at(0, 1), Some(0));
        assert_eq!(ps.cage_index_at(2, 2), None);
    }
}
