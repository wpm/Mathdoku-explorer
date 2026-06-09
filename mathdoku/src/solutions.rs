//! The [`Solutions`] iterator: MAC search over a [`Puzzle`]'s constraint graph.

use crate::Error;
use crate::fill::Fill;
use crate::polyomino::Cell;
use crate::puzzle::Puzzle;

/// An iterator over all solutions for a [`Puzzle`].
///
/// Each item is a solved [`Puzzle`] in which every cell's fill is a singleton.
/// Solutions are produced by interleaved propagation and backtracking search (MAC):
/// branching on the most-constrained cell calls [`Puzzle::set`], which propagates
/// all constraints to a fixpoint before the next branch is chosen.
///
/// Obtained via [`Puzzle::solutions`].
#[must_use]
#[allow(clippy::redundant_pub_crate)]
pub(crate) struct Solutions {
    stack: Vec<Puzzle>,
}

impl Solutions {
    pub(crate) fn new(puzzle: &Puzzle) -> Self {
        Self {
            stack: vec![puzzle.clone()],
        }
    }

    /// Finds the cell with the fewest candidate values of size ≥ 2 (most constrained).
    fn branch_cell(puzzle: &Puzzle) -> Option<(Cell, Fill)> {
        let n = puzzle.n();
        let mut best: Option<(Cell, Fill)> = None;
        for r in 1..=n {
            for c in 1..=n {
                let cell = Cell(r, c);
                if let Ok(fill) = puzzle.get(cell)
                    && fill.len() >= 2
                    && best.is_none_or(|(_, d)| fill.len() < d.len())
                {
                    best = Some((cell, fill));
                }
            }
        }
        best
    }
}

impl Iterator for Solutions {
    type Item = Result<Puzzle, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(puzzle) = self.stack.pop() {
            let n = puzzle.n();

            // Check for success: all cells' fills are singletons.
            let solved = (1..=n)
                .flat_map(|r| (1..=n).map(move |c| Cell(r, c)))
                .all(|cell| puzzle.get(cell).is_ok_and(Fill::is_singleton));
            if solved {
                return Some(Ok(puzzle));
            }

            // Branch on the most constrained unassigned cell.
            if let Some((cell, fill)) = Self::branch_cell(&puzzle) {
                for v in fill.values() {
                    if let Ok(child) = puzzle.set(cell, v)
                        && let Some(fp) = child.fixpoint()
                    {
                        self.stack.push(fp);
                    }
                }
            }
        }
        None
    }
}
