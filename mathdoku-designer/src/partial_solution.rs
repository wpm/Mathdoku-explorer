//! The cage structure and constrained cell domains for the puzzle being designed,
//! shared via context for on-demand queries.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use mathdoku::{Cage, Cell, Grid, Puzzle};

/// The cage structure and constrained cell domains for the puzzle being designed,
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
