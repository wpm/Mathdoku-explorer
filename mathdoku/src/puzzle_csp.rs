//! Wires [`Puzzle`] into the generic CSP framework from [`crate::csp`].
//!
//! The Mathdoku solving problem maps onto the CSP abstractions as follows:
//!
//! | CSP concept | Mathdoku instance |
//! |-------------|-------------------|
//! | Variable    | [`PuzzleCell`] — a single cell in a [`Puzzle`] whose domain is a [`Values`] set |
//! | Constraint  | [`AllDifferent`] — every row and column must contain distinct values |
//! | Constraint  | [`Cage`] — arithmetic target over a polyomino of cells |
//! | State       | [`Puzzle`] — holds one [`Values`] domain per cell |
//!
//! [`generalized_arc_consistency`] drives solving: it maintains a worklist
//! of constraints and propagates each in turn, re-queuing constraints adjacent to any
//! cell whose domain shrinks, until no constraint can narrow any domain further.

use crate::cage::Cage;
use crate::csp::{Constraint, Variable, generalized_arc_consistency};
use crate::puzzle::Puzzle;
use crate::regin::regin_gac;
use crate::{Cell, Error, N, Values};

/// A [`Cell`] in a [`Puzzle`], used as the CSP variable type.
///
/// Stores the cell coordinate together with the structural puzzle data — grid
/// size and cage list — needed to enumerate the constraints that mention this
/// cell. The current cell domains are not stored here; they live in the [`Puzzle`]
/// state passed to each propagation call.
struct PuzzleCell {
    cell: Cell,
    n: usize,
    cages: Vec<Cage>,
}

impl PuzzleCell {
    /// Creates a `PuzzleCell` for `cell` within `puzzle`.
    ///
    /// # Errors
    /// Returns [`Error::InvalidCell`] if `cell` is outside the grid.
    fn new(cell: Cell, puzzle: &Puzzle) -> Result<Self, Error> {
        let _ = puzzle.get_cell_values(cell)?;
        Ok(Self {
            cell,
            n: puzzle.n(),
            cages: puzzle.cages().cloned().collect(),
        })
    }
}

/// The constraint that all cells in a row or column must contain distinct values.
///
/// Stores the ordered list of cells in the row or column. Propagation runs
/// Régin's GAC algorithm (see [`crate::regin`]) over those cells' current domains.
#[derive(Clone)]
struct AllDifferent(Vec<Cell>);

impl AllDifferent {
    /// Returns an `AllDifferent` constraint for row `row` of an `n×n` grid.
    fn row(n: usize, row: usize) -> Self {
        Self((0..n).map(|column| Cell::new(row, column)).collect())
    }

    /// Returns an `AllDifferent` constraint for column `column` of an `n×n` grid.
    fn column(n: usize, column: usize) -> Self {
        Self((0..n).map(|row| Cell::new(row, column)).collect())
    }
}

/// A cell participates in one [`AllDifferent`] per row and column, plus the one [`Cage`] that covers it.
impl Variable<PuzzleConstraint> for PuzzleCell {
    fn constraints(&self) -> Vec<PuzzleConstraint> {
        let n = self.n;
        let all_different = [AllDifferent::row, AllDifferent::column]
            .iter()
            .flat_map(|f| (0..n).map(move |i| f(n, i)))
            .map(PuzzleConstraint::AllDifferent);
        let cage = self
            .cages
            .iter()
            .filter(|c| c.cells().contains(&self.cell))
            .map(|c| PuzzleConstraint::Cage(c.clone()));
        all_different.chain(cage).collect()
    }
}

/// A constraint on a [`PuzzleCell`] variable, either an [`AllDifferent`] or a [`Cage`].
#[derive(Clone)]
enum PuzzleConstraint {
    AllDifferent(AllDifferent),
    Cage(Cage),
}

/// Dispatches propagation to the inner [`AllDifferent`] or [`Cage`] constraint.
impl Constraint<Puzzle, PuzzleCell, Error> for PuzzleConstraint {
    fn propagate(&self, state: &Puzzle) -> Result<(Puzzle, Vec<PuzzleCell>), Error> {
        match self {
            Self::AllDifferent(c) => c.propagate(state),
            Self::Cage(c) => c.propagate(state),
        }
    }
}

/// Applies `new_domains` to `state`, returning the updated state and any cells whose domains changed.
fn apply_domains(
    state: &Puzzle,
    cells: &[Cell],
    old_domains: &[Values],
    new_domains: &[Values],
) -> Result<(Puzzle, Vec<PuzzleCell>), Error> {
    let mut new_state = state.clone();
    let mut changed = vec![];
    for ((&cell, old), new) in cells.iter().zip(old_domains).zip(new_domains) {
        if new != old {
            new_state = new_state.set_domain(cell, *new)?;
            changed.push(PuzzleCell::new(cell, &new_state)?);
        }
    }
    Ok((new_state, changed))
}

/// Runs Régin's GAC algorithm over the cells in this row or column.
impl Constraint<Puzzle, PuzzleCell, Error> for AllDifferent {
    fn propagate(&self, state: &Puzzle) -> Result<(Puzzle, Vec<PuzzleCell>), Error> {
        let cells = &self.0;
        let old_domains: Vec<Values> = cells
            .iter()
            .map(|&c| state.get_cell_values(c))
            .collect::<Result<_, _>>()?;
        let new_domains = regin_gac(&old_domains);
        apply_domains(state, cells, &old_domains, &new_domains)
    }
}

/// Enforces GAC on all row, column, and cage constraints, returning the fixpoint state.
///
/// Builds the full constraint list — one [`AllDifferent`] per row and column, plus one
/// [`Cage`] constraint per cage — then runs [`generalized_arc_consistency`] to a fixpoint.
///
/// # Errors
/// Returns an error if any cell is out of bounds during propagation.
pub fn puzzle_fixpoint(puzzle: &Puzzle) -> Result<Puzzle, Error> {
    let n = puzzle.n();
    let rows = (0..n).map(|r| PuzzleConstraint::AllDifferent(AllDifferent::row(n, r)));
    let cols = (0..n).map(|c| PuzzleConstraint::AllDifferent(AllDifferent::column(n, c)));
    let cages = puzzle.cages().cloned().map(PuzzleConstraint::Cage);
    let constraints: Vec<PuzzleConstraint> = rows.chain(cols).chain(cages).collect();
    generalized_arc_consistency(puzzle.clone(), &constraints)
}

/// Prunes cell domains to values that appear in at least one valid tuple for this cage's arithmetic operation.
impl Constraint<Puzzle, PuzzleCell, Error> for Cage {
    fn propagate(&self, state: &Puzzle) -> Result<(Puzzle, Vec<PuzzleCell>), Error> {
        let cells = self.cells();
        let old_domains: Vec<Values> = cells
            .iter()
            .map(|&c| state.get_cell_values(c))
            .collect::<Result<_, _>>()?;

        // A value survives at position i iff some valid tuple uses it there
        // and every other position's value is in that cell's current domain.
        let mut new_domains = vec![Values::default(); cells.len()];
        #[allow(clippy::cast_possible_truncation)]
        for tuple in self.tuples(state.n() as N) {
            if tuple
                .iter()
                .zip(&old_domains)
                .all(|(&v, domain)| domain.contains(v))
            {
                for (i, &v) in tuple.iter().enumerate() {
                    new_domains[i] = new_domains[i] | Values::new(&[v]);
                }
            }
        }

        apply_domains(state, &cells, &old_domains, &new_domains)
    }
}

/// An iterator over all solutions to a [`Puzzle`].
///
/// Each item is a solved [`Puzzle`] in which every cell domain is a singleton.
/// Solutions are produced by interleaved propagation and backtracking search (MAC):
/// after each branch, [`puzzle_fixpoint`] is called to prune as far as possible before
/// the next branch.
///
/// Obtained via [`Puzzle::solutions`].
pub struct Solutions {
    stack: Vec<Puzzle>,
}

impl Solutions {
    pub fn new(puzzle: &Puzzle) -> Self {
        Self {
            stack: vec![puzzle.clone()],
        }
    }

    /// Finds the cell with the smallest domain of size ≥ 2 (the most constrained variable).
    fn branch_cell(puzzle: &Puzzle) -> Option<(Cell, Values)> {
        let n = puzzle.n();
        let mut best: Option<(Cell, Values)> = None;
        for r in 0..n {
            for c in 0..n {
                let cell = Cell::new(r, c);
                if let Ok(domain) = puzzle.get_cell_values(cell)
                    && domain.len() >= 2
                    && best.is_none_or(|(_, d)| domain.len() < d.len())
                {
                    best = Some((cell, domain));
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
            // Propagate to fixpoint.
            let puzzle = match puzzle_fixpoint(&puzzle) {
                Ok(p) => p,
                Err(e) => return Some(Err(e)),
            };

            let n = puzzle.n();

            // Check for failure: any empty domain means this branch is dead.
            let failed = (0..n)
                .flat_map(|r| (0..n).map(move |c| Cell::new(r, c)))
                .any(|cell| puzzle.get_cell_values(cell).is_ok_and(Values::is_empty));
            if failed {
                continue;
            }

            // Check for success: all domains are singletons.
            let solved = (0..n)
                .flat_map(|r| (0..n).map(move |c| Cell::new(r, c)))
                .all(|cell| puzzle.get_cell_values(cell).is_ok_and(Values::is_singleton));
            if solved {
                return Some(Ok(puzzle));
            }

            // Branch on the most constrained unassigned cell.
            if let Some((cell, domain)) = Self::branch_cell(&puzzle) {
                for v in domain.values() {
                    if let Ok(child) = puzzle.set_cell_value(cell, v) {
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

    fn puzzle_with_domains(domains: &[(&(usize, usize), &[u8])]) -> Puzzle {
        let n = domains
            .iter()
            .map(|((r, c), _)| r.max(c) + 1)
            .max()
            .unwrap();
        let mut p = Puzzle::new(n).unwrap();
        for ((r, c), vals) in domains {
            p = p.set_domain(Cell::new(*r, *c), Values::new(vals)).unwrap();
        }
        p
    }

    fn changed_cells(changed: &[PuzzleCell]) -> Vec<Cell> {
        changed.iter().map(|pc| pc.cell).collect()
    }

    // Puzzle with row 0 partially constrained: (0,0)={1,2}, (0,1)={2}, (0,2)={1,3}.
    // Régin should force (0,0)→{1} and (0,2)→{3}.
    fn row0_forced_puzzle() -> Puzzle {
        puzzle_with_domains(&[(&(0, 0), &[1, 2]), (&(0, 1), &[2]), (&(0, 2), &[1, 3])])
    }

    // --- PuzzleCell::new ---

    #[test]
    fn new_valid_cell_succeeds() {
        let p = Puzzle::new(3).unwrap();
        assert!(PuzzleCell::new(Cell::new(2, 2), &p).is_ok());
    }

    #[test]
    fn new_out_of_bounds_returns_invalid_cell() {
        let p = Puzzle::new(3).unwrap();
        assert!(matches!(
            PuzzleCell::new(Cell::new(3, 0), &p),
            Err(Error::InvalidCell(_))
        ));
    }

    // --- AllDifferent::propagate ---

    #[test]
    fn propagate_full_domains_unchanged() {
        let p = Puzzle::new(3).unwrap();
        let (new_p, changed) = AllDifferent::row(3, 0).propagate(&p).unwrap();
        assert_eq!(new_p, p);
        assert!(changed.is_empty());
    }

    #[test]
    fn propagate_prunes_forced_value() {
        let (new_p, changed) = AllDifferent::row(3, 0)
            .propagate(&row0_forced_puzzle())
            .unwrap();
        assert_eq!(
            new_p.get_cell_values(Cell::new(0, 0)).unwrap(),
            Values::new(&[1])
        );
        assert_eq!(
            new_p.get_cell_values(Cell::new(0, 1)).unwrap(),
            Values::new(&[2])
        );
        assert_eq!(
            new_p.get_cell_values(Cell::new(0, 2)).unwrap(),
            Values::new(&[3])
        );
        let cells = changed_cells(&changed);
        assert_eq!(cells.len(), 2);
        assert!(cells.contains(&Cell::new(0, 0)));
        assert!(cells.contains(&Cell::new(0, 2)));
    }

    #[test]
    fn propagate_infeasible_empties_domains() {
        let p = puzzle_with_domains(&[(&(0, 0), &[1]), (&(1, 0), &[1])]);
        let (new_p, changed) = AllDifferent::column(2, 0).propagate(&p).unwrap();
        assert!(new_p.get_cell_values(Cell::new(0, 0)).unwrap().is_empty());
        assert!(new_p.get_cell_values(Cell::new(1, 0)).unwrap().is_empty());
        assert_eq!(changed.len(), 2);
    }

    #[test]
    fn propagate_unchanged_cells_not_in_changed() {
        let (_, changed) = AllDifferent::row(3, 0)
            .propagate(&row0_forced_puzzle())
            .unwrap();
        assert!(!changed_cells(&changed).contains(&Cell::new(0, 1)));
    }

    #[test]
    fn propagate_column_constraint() {
        // (0,1) pins 1, forcing (1,1)→{2} and (2,1)→{3}.
        let p = puzzle_with_domains(&[(&(0, 1), &[1]), (&(1, 1), &[1, 2]), (&(2, 1), &[2, 3])]);
        let (new_p, _) = AllDifferent::column(3, 1).propagate(&p).unwrap();
        assert_eq!(
            new_p.get_cell_values(Cell::new(1, 1)).unwrap(),
            Values::new(&[2])
        );
        assert_eq!(
            new_p.get_cell_values(Cell::new(2, 1)).unwrap(),
            Values::new(&[3])
        );
    }

    // --- Cage::propagate ---

    fn cage(
        positions: &[(usize, usize)],
        operator: crate::cage::Operator,
        target: crate::M,
    ) -> Cage {
        use crate::cage::Operation;
        use crate::polyomino::Polyomino;
        let cells: Vec<Cell> = positions.iter().map(|&(r, c)| Cell::new(r, c)).collect();
        Cage::new(
            Polyomino::from_cells(&cells).unwrap(),
            Operation::new(operator, target),
        )
    }

    #[test]
    fn cage_propagate_given_pins_cell() {
        // A Given cage at (0,0) with target 3 in a 4×4 puzzle:
        // (0,0) should be pruned to {3} regardless of its initial domain.
        let p = Puzzle::new(4).unwrap();
        let c = cage(&[(0, 0)], crate::cage::Operator::Given, 3);
        let (new_p, changed) = c.propagate(&p).unwrap();
        assert_eq!(
            new_p.get_cell_values(Cell::new(0, 0)).unwrap(),
            Values::new(&[3])
        );
        assert_eq!(changed_cells(&changed), vec![Cell::new(0, 0)]);
    }

    #[test]
    fn cage_propagate_add_pair_prunes_impossible_values() {
        // Add a cage over (0,0) and (0,1), target=3, in a 4×4 puzzle.
        // Valid tuples: (1,2) and (2,1). So (0,0) and (0,1) are both pruned to {1,2}.
        let p = Puzzle::new(4).unwrap();
        let c = cage(&[(0, 0), (0, 1)], crate::cage::Operator::Add, 3);
        let (new_p, _) = c.propagate(&p).unwrap();
        assert_eq!(
            new_p.get_cell_values(Cell::new(0, 0)).unwrap(),
            Values::new(&[1, 2])
        );
        assert_eq!(
            new_p.get_cell_values(Cell::new(0, 1)).unwrap(),
            Values::new(&[1, 2])
        );
    }

    #[test]
    fn cage_propagate_no_valid_tuple_empties_domains() {
        // Add a cage over (0,0) and (0,1), target=3, but both cells are pinned to {4}.
        // No valid tuple exists, so both domains should become empty.
        let p = puzzle_with_domains(&[(&(0, 0), &[4]), (&(0, 1), &[4])]);
        let c = cage(&[(0, 0), (0, 1)], crate::cage::Operator::Add, 3);
        let (new_p, changed) = c.propagate(&p).unwrap();
        assert!(new_p.get_cell_values(Cell::new(0, 0)).unwrap().is_empty());
        assert!(new_p.get_cell_values(Cell::new(0, 1)).unwrap().is_empty());
        assert_eq!(changed.len(), 2);
    }

    #[test]
    fn cage_propagate_domain_constrains_tuples() {
        // Add a cage over (0,0) and (0,1), target=5, in a 4×4 puzzle.
        // Valid tuples without domain constraints: (1,4),(4,1),(2,3),(3,2).
        // Pin (0,1) to {1,2}: surviving tuples are (4,1) and (3,2).
        // So (0,0) is pruned to {3,4} and (0,1) stays {1,2}.
        let p = Puzzle::new(4)
            .unwrap()
            .set_domain(Cell::new(0, 1), Values::new(&[1, 2]))
            .unwrap();
        let c = cage(&[(0, 0), (0, 1)], crate::cage::Operator::Add, 5);
        let (new_p, _) = c.propagate(&p).unwrap();
        assert_eq!(
            new_p.get_cell_values(Cell::new(0, 0)).unwrap(),
            Values::new(&[3, 4])
        );
        assert_eq!(
            new_p.get_cell_values(Cell::new(0, 1)).unwrap(),
            Values::new(&[1, 2])
        );
    }
}
