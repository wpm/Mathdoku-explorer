//! Wires [`Grid`] and [`Puzzle`] into the generic CSP framework from [`crate::csp`].
//!
//! The Mathdoku solving problem maps onto the CSP abstractions as follows:
//!
//! | CSP concept | Mathdoku instance |
//! |-------------|-------------------|
//! | Variable    | `PuzzleCell` — a single cell in a [`Grid`] whose values are a [`Values`] set |
//! | Constraint  | `AllDifferent` — every row and column must contain distinct values |
//! | Constraint  | [`Cage`] — arithmetic target over a polyomino of cells |
//! | State       | [`Grid`] — holds one [`Values`] set per cell |
//!
//! [`crate::csp::generalized_arc_consistency`] drives solving: it maintains a worklist
//! of constraints and propagates each in turn, re-queuing constraints adjacent to any
//! cell whose values shrink, until no constraint can narrow any cell's values further.
//!
//! Row and column all-different is enforced via [`crate::regin`]; cage constraint
//! propagation uses [`Mdd::support`](crate::Mdd::support).

use std::sync::Arc;

use crate::cage::Cage;
use crate::csp::{Constraint, Variable, generalized_arc_consistency};
use crate::grid::Grid;
use crate::puzzle::Puzzle;
use crate::regin::regin_gac;
use crate::{Cell, Error, Values};

/// A [`Cell`] in a [`Grid`], used as the CSP variable type.
///
/// Stores the cell coordinate together with the structural puzzle data — grid
/// size and cage list — needed to enumerate the constraints that mention this
/// cell. The current cell values are not stored here; they live in the [`Grid`]
/// state passed to each propagation call.
struct PuzzleCell {
    cell: Cell,
    n: usize,
    cages: Arc<Vec<Cage>>,
}

impl PuzzleCell {
    /// Creates a `PuzzleCell` for `cell` within `grid` and `puzzle`.
    ///
    /// # Errors
    /// Returns [`Error::InvalidCell`] if `cell` is outside the grid.
    fn new(cell: Cell, grid: &Grid, cages: Arc<Vec<Cage>>) -> Result<Self, Error> {
        let _ = grid.cell_values(cell)?;
        Ok(Self {
            cell,
            n: grid.n(),
            cages,
        })
    }
}

/// The constraint that all cells in a row or column must contain distinct values.
///
/// Stores the ordered list of cells in the row or column. Propagation runs
/// Régin's GAC algorithm (see [`crate::regin`]) over those cells' current values.
#[derive(Clone)]
struct AllDifferent {
    cells: Vec<Cell>,
    cages: Arc<Vec<Cage>>,
}

impl AllDifferent {
    /// Returns an `AllDifferent` constraint for row `row` of an `n×n` grid.
    fn row(n: usize, row: usize, cages: Arc<Vec<Cage>>) -> Self {
        Self {
            cells: (0..n).map(|column| Cell::new(row, column)).collect(),
            cages,
        }
    }

    /// Returns an `AllDifferent` constraint for column `column` of an `n×n` grid.
    fn column(n: usize, column: usize, cages: Arc<Vec<Cage>>) -> Self {
        Self {
            cells: (0..n).map(|row| Cell::new(row, column)).collect(),
            cages,
        }
    }
}

/// A cell participates in one [`AllDifferent`] per row and column, plus the one [`Cage`] that
/// covers it.
impl Variable<PuzzleConstraint> for PuzzleCell {
    fn constraints(&self) -> Vec<PuzzleConstraint> {
        let n = self.n;
        let cages = Arc::clone(&self.cages);
        let all_different = [
            |n, i, c: Arc<Vec<Cage>>| AllDifferent::row(n, i, c),
            |n, i, c: Arc<Vec<Cage>>| AllDifferent::column(n, i, c),
        ]
        .iter()
        .flat_map(|f| {
            let cages = Arc::clone(&cages);
            (0..n).map(move |i| f(n, i, Arc::clone(&cages)))
        })
        .map(PuzzleConstraint::AllDifferent);
        let cage_cages = Arc::clone(&self.cages);
        let cage = self
            .cages
            .iter()
            .filter(|c| c.cells().contains(&self.cell))
            .map(move |c| PuzzleConstraint::Cage(c.clone(), Arc::clone(&cage_cages)));
        all_different.chain(cage).collect()
    }
}

/// A constraint on a [`PuzzleCell`] variable, either an [`AllDifferent`] or a [`Cage`].
#[derive(Clone)]
enum PuzzleConstraint {
    AllDifferent(AllDifferent),
    Cage(Cage, Arc<Vec<Cage>>),
}

/// Dispatches propagation to the inner [`AllDifferent`] or [`Cage`] constraint.
impl Constraint<Grid, PuzzleCell, Error> for PuzzleConstraint {
    fn propagate(&self, state: &Grid) -> Result<(Grid, Vec<PuzzleCell>), Error> {
        match self {
            Self::AllDifferent(c) => c.propagate(state),
            Self::Cage(c, cages) => propagate_cage(c, cages, state),
        }
    }
}

/// Applies `new_values` to `state`, returning the updated state and any cells whose values
/// changed.
fn apply_values(
    state: &Grid,
    cages: &Arc<Vec<Cage>>,
    cells: &[Cell],
    old_values: &[Values],
    new_values: &[Values],
) -> Result<(Grid, Vec<PuzzleCell>), Error> {
    let mut new_state = state.clone();
    let mut changed = vec![];
    for ((&cell, old), new) in cells.iter().zip(old_values).zip(new_values) {
        if new != old {
            new_state = new_state.set_values(cell, *new)?;
            changed.push(PuzzleCell::new(cell, &new_state, Arc::clone(cages))?);
        }
    }
    Ok((new_state, changed))
}

/// Runs Régin's GAC algorithm over the cells in this row or column.
impl Constraint<Grid, PuzzleCell, Error> for AllDifferent {
    fn propagate(&self, state: &Grid) -> Result<(Grid, Vec<PuzzleCell>), Error> {
        let cells = &self.cells;
        let old_values: Vec<Values> = cells
            .iter()
            .map(|&c| state.cell_values(c))
            .collect::<Result<_, _>>()?;
        let new_values = regin_gac(&old_values);
        apply_values(state, &self.cages, cells, &old_values, &new_values)
    }
}

/// Prunes cell values to those that appear in at least one valid tuple for this cage's arithmetic
/// operation, computed as the GAC support of the cage's [`Mdd`](crate::Mdd).
fn propagate_cage(
    cage: &Cage,
    cages: &Arc<Vec<Cage>>,
    state: &Grid,
) -> Result<(Grid, Vec<PuzzleCell>), Error> {
    let cells = cage.cells();
    let old_values: Vec<Values> = cells
        .iter()
        .map(|&c| state.cell_values(c))
        .collect::<Result<_, _>>()?;
    let new_values = cage.mdd(state.n()).support(&old_values);
    apply_values(state, cages, &cells, &old_values, &new_values)
}

/// Enforces GAC on all row, column, and cage constraints, returning the fixpoint state.
///
/// Builds the full constraint list — one [`AllDifferent`] per row and column, plus one
/// [`Cage`] constraint per cage — then runs [`generalized_arc_consistency`] to a fixpoint.
///
/// # Errors
/// Returns an error if any cell is out of bounds during propagation.
pub fn grid_fixpoint(grid: &Grid, puzzle: &Puzzle) -> Result<Grid, Error> {
    let n = grid.n();
    let cages: Arc<Vec<Cage>> = Arc::new(puzzle.cages().cloned().collect());
    let rows =
        (0..n).map(|r| PuzzleConstraint::AllDifferent(AllDifferent::row(n, r, Arc::clone(&cages))));
    let cols = (0..n)
        .map(|c| PuzzleConstraint::AllDifferent(AllDifferent::column(n, c, Arc::clone(&cages))));
    let cage_constraints = puzzle
        .cages()
        .cloned()
        .map(|cage| PuzzleConstraint::Cage(cage, Arc::clone(&cages)));
    let constraints: Vec<PuzzleConstraint> = rows.chain(cols).chain(cage_constraints).collect();
    generalized_arc_consistency(grid.clone(), &constraints)
}

/// An iterator over all solutions for a [`Grid`] under a [`Puzzle`]'s constraints.
///
/// Each item is a solved [`Grid`] in which every cell's values are a singleton.
/// Solutions are produced by interleaved propagation and backtracking search (MAC):
/// after each branch, [`grid_fixpoint`] is called to prune as far as possible before
/// the next branch.
///
/// Obtained via [`Grid::solutions`].
pub struct Solutions<'a> {
    stack: Vec<Grid>,
    puzzle: &'a Puzzle,
}

impl<'a> Solutions<'a> {
    pub fn new(grid: &Grid, puzzle: &'a Puzzle) -> Self {
        Self {
            stack: vec![grid.clone()],
            puzzle,
        }
    }

    /// Finds the cell with the fewest values of size ≥ 2 (the most constrained variable).
    fn branch_cell(grid: &Grid) -> Option<(Cell, Values)> {
        let n = grid.n();
        let mut best: Option<(Cell, Values)> = None;
        for r in 0..n {
            for c in 0..n {
                let cell = Cell::new(r, c);
                if let Ok(values) = grid.cell_values(cell)
                    && values.len() >= 2
                    && best.is_none_or(|(_, d)| values.len() < d.len())
                {
                    best = Some((cell, values));
                }
            }
        }
        best
    }
}

impl Iterator for Solutions<'_> {
    type Item = Result<Grid, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(grid) = self.stack.pop() {
            // Propagate to fixpoint.
            let grid = match grid_fixpoint(&grid, self.puzzle) {
                Ok(g) => g,
                Err(e) => return Some(Err(e)),
            };

            let n = grid.n();

            // Check for failure: any empty value set means this branch is dead.
            let failed = (0..n)
                .flat_map(|r| (0..n).map(move |c| Cell::new(r, c)))
                .any(|cell| grid.cell_values(cell).is_ok_and(Values::is_empty));
            if failed {
                continue;
            }

            // Check for success: all cells' values are singletons.
            let solved = (0..n)
                .flat_map(|r| (0..n).map(move |c| Cell::new(r, c)))
                .all(|cell| grid.cell_values(cell).is_ok_and(Values::is_singleton));
            if solved {
                return Some(Ok(grid));
            }

            // Branch on the most constrained unassigned cell.
            if let Some((cell, values)) = Self::branch_cell(&grid) {
                for v in values.values() {
                    if let Ok(child) = grid.set_cell_value(cell, v) {
                        self.stack.push(child);
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::csp::Constraint;

    fn grid_with_values(values: &[(&(usize, usize), &[u8])]) -> Grid {
        let n = values.iter().map(|((r, c), _)| r.max(c) + 1).max().unwrap();
        let mut g = Grid::new(n).unwrap();
        for ((r, c), vals) in values {
            g = g
                .set_values(Cell::new(*r, *c), Values::new(vals).unwrap())
                .unwrap();
        }
        g
    }

    fn changed_cells(changed: &[PuzzleCell]) -> Vec<Cell> {
        changed.iter().map(|pc| pc.cell).collect()
    }

    // Grid with row 0 partially constrained: (0,0)={1,2}, (0,1)={2}, (0,2)={1,3}.
    // Régin should force (0,0)→{1} and (0,2)→{3}.
    fn row0_forced_grid() -> Grid {
        grid_with_values(&[(&(0, 0), &[1, 2]), (&(0, 1), &[2]), (&(0, 2), &[1, 3])])
    }

    fn empty_cages() -> Arc<Vec<Cage>> {
        Arc::new(vec![])
    }

    fn all_different_row(n: usize, row: usize) -> AllDifferent {
        AllDifferent::row(n, row, empty_cages())
    }

    fn all_different_column(n: usize, col: usize) -> AllDifferent {
        AllDifferent::column(n, col, empty_cages())
    }

    // --- PuzzleCell::new ---

    #[test]
    fn new_valid_cell_succeeds() {
        let g = Grid::new(3).unwrap();
        assert!(PuzzleCell::new(Cell::new(2, 2), &g, empty_cages()).is_ok());
    }

    #[test]
    fn new_out_of_bounds_returns_invalid_cell() {
        let g = Grid::new(3).unwrap();
        assert!(matches!(
            PuzzleCell::new(Cell::new(3, 0), &g, empty_cages()),
            Err(Error::InvalidCell(_))
        ));
    }

    // --- AllDifferent::propagate ---

    #[test]
    fn propagate_full_values_unchanged() {
        let g = Grid::new(3).unwrap();
        let (new_g, changed) = all_different_row(3, 0).propagate(&g).unwrap();
        assert_eq!(new_g, g);
        assert!(changed.is_empty());
    }

    #[test]
    fn propagate_prunes_forced_value() {
        let (new_g, changed) = all_different_row(3, 0)
            .propagate(&row0_forced_grid())
            .unwrap();
        assert_eq!(
            new_g.cell_values(Cell::new(0, 0)).unwrap(),
            Values::new(&[1]).unwrap()
        );
        assert_eq!(
            new_g.cell_values(Cell::new(0, 1)).unwrap(),
            Values::new(&[2]).unwrap()
        );
        assert_eq!(
            new_g.cell_values(Cell::new(0, 2)).unwrap(),
            Values::new(&[3]).unwrap()
        );
        let cells = changed_cells(&changed);
        assert_eq!(cells.len(), 2);
        assert!(cells.contains(&Cell::new(0, 0)));
        assert!(cells.contains(&Cell::new(0, 2)));
    }

    #[test]
    fn propagate_infeasible_empties_values() {
        let g = grid_with_values(&[(&(0, 0), &[1]), (&(1, 0), &[1])]);
        let (new_g, changed) = all_different_column(2, 0).propagate(&g).unwrap();
        assert!(new_g.cell_values(Cell::new(0, 0)).unwrap().is_empty());
        assert!(new_g.cell_values(Cell::new(1, 0)).unwrap().is_empty());
        assert_eq!(changed.len(), 2);
    }

    #[test]
    fn propagate_unchanged_cells_not_in_changed() {
        let (_, changed) = all_different_row(3, 0)
            .propagate(&row0_forced_grid())
            .unwrap();
        assert!(!changed_cells(&changed).contains(&Cell::new(0, 1)));
    }

    #[test]
    fn propagate_column_constraint() {
        // (0,1) pins 1, forcing (1,1)→{2} and (2,1)→{3}.
        let g = grid_with_values(&[(&(0, 1), &[1]), (&(1, 1), &[1, 2]), (&(2, 1), &[2, 3])]);
        let (new_g, _) = all_different_column(3, 1).propagate(&g).unwrap();
        assert_eq!(
            new_g.cell_values(Cell::new(1, 1)).unwrap(),
            Values::new(&[2]).unwrap()
        );
        assert_eq!(
            new_g.cell_values(Cell::new(2, 1)).unwrap(),
            Values::new(&[3]).unwrap()
        );
    }

    // --- Cage::propagate (via propagate_cage) ---

    fn cage(
        positions: &[(usize, usize)],
        operator: crate::Operator,
        target: crate::Target,
    ) -> Cage {
        use crate::operation::Operation;
        use crate::polyomino::Polyomino;
        let cells: Vec<Cell> = positions.iter().map(|&(r, c)| Cell::new(r, c)).collect();
        Cage::new(
            Polyomino::from_cells(&cells).unwrap(),
            Operation::new(operator, target),
        )
    }

    #[test]
    fn cage_propagate_given_pins_cell() {
        // A Given cage at (0,0) with target 3 in a 4×4 grid:
        // (0,0) should be pruned to {3} regardless of its initial values.
        let g = Grid::new(4).unwrap();
        let c = cage(&[(0, 0)], crate::Operator::Given, 3);
        let (new_g, changed) = propagate_cage(&c, &empty_cages(), &g).unwrap();
        assert_eq!(
            new_g.cell_values(Cell::new(0, 0)).unwrap(),
            Values::new(&[3]).unwrap()
        );
        assert_eq!(changed_cells(&changed), vec![Cell::new(0, 0)]);
    }

    #[test]
    fn cage_propagate_add_pair_prunes_impossible_values() {
        // Add a cage over (0,0) and (0,1), target=3, in a 4×4 grid.
        // Valid tuples: (1,2) and (2,1). So (0,0) and (0,1) are both pruned to {1,2}.
        let g = Grid::new(4).unwrap();
        let c = cage(&[(0, 0), (0, 1)], crate::Operator::Add, 3);
        let (new_g, _) = propagate_cage(&c, &empty_cages(), &g).unwrap();
        assert_eq!(
            new_g.cell_values(Cell::new(0, 0)).unwrap(),
            Values::new(&[1, 2]).unwrap()
        );
        assert_eq!(
            new_g.cell_values(Cell::new(0, 1)).unwrap(),
            Values::new(&[1, 2]).unwrap()
        );
    }

    #[test]
    fn cage_propagate_no_valid_tuple_empties_values() {
        // Add a cage over (0,0) and (0,1), target=3, but both cells are pinned to {4}.
        // No valid tuple exists, so both cells' values should become empty.
        let g = grid_with_values(&[(&(0, 0), &[4]), (&(0, 1), &[4])]);
        let c = cage(&[(0, 0), (0, 1)], crate::Operator::Add, 3);
        let (new_g, changed) = propagate_cage(&c, &empty_cages(), &g).unwrap();
        assert!(new_g.cell_values(Cell::new(0, 0)).unwrap().is_empty());
        assert!(new_g.cell_values(Cell::new(0, 1)).unwrap().is_empty());
        assert_eq!(changed.len(), 2);
    }

    #[test]
    fn cage_propagate_values_constrain_tuples() {
        // Add a cage over (0,0) and (0,1), target=5, in a 4×4 grid.
        // Valid tuples without value constraints: (1,4),(4,1),(2,3),(3,2).
        // Pin (0,1) to {1,2}: surviving tuples are (4,1) and (3,2).
        // So (0,0) is pruned to {3,4} and (0,1) stays {1,2}.
        let g = Grid::new(4)
            .unwrap()
            .set_values(Cell::new(0, 1), Values::new(&[1, 2]).unwrap())
            .unwrap();
        let c = cage(&[(0, 0), (0, 1)], crate::Operator::Add, 5);
        let (new_g, _) = propagate_cage(&c, &empty_cages(), &g).unwrap();
        assert_eq!(
            new_g.cell_values(Cell::new(0, 0)).unwrap(),
            Values::new(&[3, 4]).unwrap()
        );
        assert_eq!(
            new_g.cell_values(Cell::new(0, 1)).unwrap(),
            Values::new(&[1, 2]).unwrap()
        );
    }
}
