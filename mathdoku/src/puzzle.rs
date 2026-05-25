//! The [`Puzzle`] type: an `n×n` grid with cage constraints.

use crate::Error::InvalidGridSize;
use crate::cage::Cage;
use crate::polyomino::Polyomino;
use crate::{Cell, Error, N, Values};

/// An `n×n` Mathdoku grid.
///
/// Stores one [`Values`] domain per cell and the list of cages that have been added.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Puzzle {
    n: usize,
    values: Box<[Values]>,
    cages: Vec<Cage>,
}

impl Puzzle {
    /// Creates an empty `n×n` puzzle with all cell domains initialized to `{1, ..., n}`.
    ///
    /// # Errors
    /// Returns [`InvalidGridSize`] if `n` is not in `1..=9`.
    pub fn new(n: usize) -> Result<Self, Error> {
        if !(1..=9).contains(&n) {
            return Err(InvalidGridSize(n));
        }
        Ok(Self {
            n,
            values: vec![Values::all(n); n * n].into_boxed_slice(),
            cages: Vec::new(),
        })
    }

    pub(crate) const fn n(&self) -> usize {
        self.n
    }

    /// Returns an iterator over all solutions to this puzzle.
    ///
    /// Each item is a solved [`Puzzle`] where every cell domain is a singleton.
    /// Uses MAC (Maintaining Arc Consistency): each branch is followed immediately by
    /// [`fixpoint`] propagation before the next branch is chosen.
    ///
    /// The iterator yields [`Err`] and stops if a propagation error occurs (e.g. an
    /// out-of-bounds cell). Well-formed puzzles will never error.
    ///
    /// [`fixpoint`]: Puzzle::fixpoint
    pub fn solutions(&self) -> impl Iterator<Item = Result<Self, Error>> {
        crate::puzzle_csp::Solutions::new(self)
    }

    /// Propagates all constraints to a fixpoint using generalized arc consistency.
    ///
    /// Runs Régin's GAC on every row and column (all-different) and every cage,
    /// re-propagating any constraint adjacent to a cell whose domain shrinks, until
    /// no further pruning is possible.
    ///
    /// # Errors
    /// Returns an error if any cell is out of bounds during propagation.
    pub fn fixpoint(&self) -> Result<Self, Error> {
        crate::puzzle_csp::puzzle_fixpoint(self)
    }

    /// Returns the current domain of every cell covered by `polyomino`, in
    /// row-major order.
    ///
    /// # Errors
    /// Returns [`Error::InvalidCell`] if any cell of `polyomino` is outside the grid.
    pub fn get_polyomino_values(&self, polyomino: &Polyomino) -> Result<Vec<Values>, Error> {
        polyomino
            .cells()
            .into_iter()
            .map(|cell| self.get_cell_values(cell))
            .collect()
    }

    /// Returns the current domain of `cell`.
    ///
    /// # Errors
    /// Returns [`Error::InvalidCell`] if `cell` is outside the grid.
    pub fn get_cell_values(&self, cell: Cell) -> Result<Values, Error> {
        Ok(self.values[self.index(cell)?])
    }

    /// Returns a new puzzle with `cell`'s domain replaced by `values`.
    ///
    /// # Errors
    /// Returns [`Error::InvalidCell`] if `cell` is outside the grid.
    pub(crate) fn set_domain(&self, cell: Cell, values: Values) -> Result<Self, Error> {
        let i = self.index(cell)?;
        let mut new_values = self.values.clone();
        new_values[i] = values;
        Ok(Self {
            n: self.n,
            values: new_values,
            cages: self.cages.clone(),
        })
    }

    /// Returns a new puzzle with `cell`'s domain narrowed to the singleton `{n}`.
    ///
    /// # Errors
    /// Returns [`Error::InvalidCell`] if `cell` is outside the grid.
    pub fn set_cell_value(&self, cell: Cell, n: N) -> Result<Self, Error> {
        let i = self.index(cell)?;
        let mut values = self.values.clone();
        values[i] = Values::new(&[n]);
        Ok(Self {
            n: self.n,
            values,
            cages: self.cages.clone(),
        })
    }

    /// Returns the cages in this puzzle in polyomino order.
    pub fn cages(&self) -> impl Iterator<Item = &Cage> {
        let mut sorted: Vec<&Cage> = self.cages.iter().collect();
        sorted.sort();
        sorted.into_iter()
    }

    /// Returns a new puzzle with `cage` appended to the cage list.
    pub fn insert_cage(&self, cage: Cage) -> Self {
        let mut cages = self.cages.clone();
        cages.push(cage);
        Self {
            n: self.n,
            values: self.values.clone(),
            cages,
        }
    }

    const fn index(&self, cell: Cell) -> Result<usize, Error> {
        if cell.row < self.n && cell.column < self.n {
            Ok(cell.row * self.n + cell.column)
        } else {
            Err(Error::InvalidCell(cell))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::M;
    use crate::cage::{Cage, Operation, Operator};
    use crate::polyomino::Polyomino;

    fn cage_at(positions: &[(usize, usize)], operator: Operator, target: M) -> Cage {
        let cells: Vec<Cell> = positions.iter().map(|&(r, c)| Cell::new(r, c)).collect();
        let poly = Polyomino::from_cells(&cells).unwrap();
        Cage::new(poly, Operation::new(operator, target))
    }

    // --- Puzzle::new ---

    #[test]
    fn new_valid_sizes_succeed() {
        for n in 1..=9 {
            assert!(Puzzle::new(n).is_ok(), "size {n} should succeed");
        }
    }

    #[test]
    fn new_size_zero_returns_err() {
        assert!(matches!(Puzzle::new(0), Err(InvalidGridSize(0))));
    }

    #[test]
    fn new_size_ten_returns_err() {
        assert!(matches!(Puzzle::new(10), Err(InvalidGridSize(10))));
    }

    #[test]
    fn new_domains_are_full() {
        let p = Puzzle::new(4).unwrap();
        let expected = Values::all(4);
        for r in 0..4 {
            for c in 0..4 {
                assert_eq!(
                    p.get_cell_values(Cell::new(r, c)).unwrap(),
                    expected,
                    "cell ({r},{c}) should have full domain"
                );
            }
        }
    }

    // --- Puzzle::get_cell_values ---

    #[test]
    fn get_cell_values_out_of_bounds_returns_err() {
        let p = Puzzle::new(3).unwrap();
        assert!(matches!(
            p.get_cell_values(Cell::new(3, 0)),
            Err(Error::InvalidCell(_))
        ));
        assert!(matches!(
            p.get_cell_values(Cell::new(0, 3)),
            Err(Error::InvalidCell(_))
        ));
    }

    // --- Puzzle::get_polyomino_values ---

    #[test]
    fn get_polyomino_values_returns_domains_in_row_major_order() {
        let p = Puzzle::new(4).unwrap();
        let poly = Polyomino::from_cells(&[Cell::new(0, 0), Cell::new(0, 1)]).unwrap();
        let vals = p.get_polyomino_values(&poly).unwrap();
        assert_eq!(vals.len(), 2);
        assert_eq!(vals[0], Values::all(4));
        assert_eq!(vals[1], Values::all(4));
    }

    #[test]
    fn get_polyomino_values_out_of_bounds_returns_err() {
        let p = Puzzle::new(3).unwrap();
        let poly_bad = Polyomino::from_cells(&[Cell::new(0, 0), Cell::new(0, 1)])
            .unwrap()
            .insert(Cell::new(0, 2))
            .unwrap();
        // All cells 0..2 are in-bounds for n=3; check something out of range.
        let out = Polyomino::from_cells(&[Cell::new(2, 2)]).unwrap();
        assert!(p.get_polyomino_values(&out).is_ok()); // (2,2) is in-bounds for n=3
        let p2 = Puzzle::new(2).unwrap();
        assert!(matches!(
            p2.get_polyomino_values(&poly_bad),
            Err(Error::InvalidCell(_))
        ));
    }

    // --- Puzzle::set_cell_values ---

    #[test]
    fn set_cell_values_narrows_domain() {
        let p = Puzzle::new(4).unwrap();
        let cell = Cell::new(1, 2);
        let p2 = p.set_cell_value(cell, 3).unwrap();
        assert_eq!(p2.get_cell_values(cell).unwrap(), Values::new(&[3]));
    }

    #[test]
    fn set_cell_values_is_non_destructive() {
        let p = Puzzle::new(4).unwrap();
        let cell = Cell::new(0, 0);
        let _ = p.set_cell_value(cell, 2).unwrap();
        // Original puzzle is unchanged.
        assert_eq!(p.get_cell_values(cell).unwrap(), Values::all(4));
    }

    #[test]
    fn set_cell_values_out_of_bounds_returns_err() {
        let p = Puzzle::new(3).unwrap();
        assert!(matches!(
            p.set_cell_value(Cell::new(3, 0), 1),
            Err(Error::InvalidCell(_))
        ));
    }

    // --- Puzzle::insert_cage ---

    #[test]
    fn insert_cage_returns_puzzle() {
        let p = Puzzle::new(4).unwrap();
        let cage = cage_at(&[(0, 0)], Operator::Given, 3);
        let p2 = p.insert_cage(cage);
        assert_eq!(p2.n(), 4);
    }

    #[test]
    fn insert_cage_is_non_destructive() {
        let p = Puzzle::new(4).unwrap();
        let cage = cage_at(&[(0, 0)], Operator::Given, 3);
        let _ = p.insert_cage(cage);
        // Original puzzle unchanged — still has no cages (domains still full).
        assert_eq!(p.get_cell_values(Cell::new(0, 0)).unwrap(), Values::all(4));
    }

    // --- Puzzle::fixpoint ---

    // Builds a fully caged 2×2 puzzle with four Given cages and verifies that
    // fixpoint pins every cell to its given value.
    //
    //   [1][2]
    //   [2][1]
    //
    // Four Given cages: (0,0)=1, (0,1)=2, (1,0)=2, (1,1)=1.
    fn solved_2x2() -> Puzzle {
        Puzzle::new(2)
            .unwrap()
            .insert_cage(cage_at(&[(0, 0)], Operator::Given, 1))
            .insert_cage(cage_at(&[(0, 1)], Operator::Given, 2))
            .insert_cage(cage_at(&[(1, 0)], Operator::Given, 2))
            .insert_cage(cage_at(&[(1, 1)], Operator::Given, 1))
    }

    #[test]
    fn fixpoint_given_cages_pin_all_cells() {
        let fp = solved_2x2().fixpoint().unwrap();
        assert_eq!(
            fp.get_cell_values(Cell::new(0, 0)).unwrap(),
            Values::new(&[1])
        );
        assert_eq!(
            fp.get_cell_values(Cell::new(0, 1)).unwrap(),
            Values::new(&[2])
        );
        assert_eq!(
            fp.get_cell_values(Cell::new(1, 0)).unwrap(),
            Values::new(&[2])
        );
        assert_eq!(
            fp.get_cell_values(Cell::new(1, 1)).unwrap(),
            Values::new(&[1])
        );
    }

    #[test]
    fn fixpoint_is_idempotent() {
        let p = solved_2x2();
        let fp1 = p.fixpoint().unwrap();
        let fp2 = fp1.fixpoint().unwrap();
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn fixpoint_no_cages_unchanged() {
        // Without cages, AllDifferent alone cannot pin any domain in a fresh puzzle.
        let p = Puzzle::new(2).unwrap();
        let fp = p.fixpoint().unwrap();
        assert_eq!(fp, p);
    }

    // 2×2 with two arithmetic cages.
    //
    //   [?][?]     (0,0)+(0,1) = 3 (Add)    → only (1,2) or (2,1)
    //   [?][?]     (1,0)÷(1,1) = 2 (Divide) → only (1,2) or (2,1)
    //
    // Both solutions [[1,2],[2,1]] and [[2,1],[1,2]] satisfy all constraints, so
    // fixpoint cannot pin any cell — but it can prune each domain to {1,2}.
    #[test]
    fn fixpoint_arithmetic_cages_prune_2x2() {
        let p = Puzzle::new(2)
            .unwrap()
            .insert_cage(cage_at(&[(0, 0), (0, 1)], Operator::Add, 3))
            .insert_cage(cage_at(&[(1, 0), (1, 1)], Operator::Divide, 2));
        let fp = p.fixpoint().unwrap();
        let expected = Values::new(&[1, 2]);
        for r in 0..2 {
            for c in 0..2 {
                assert_eq!(
                    fp.get_cell_values(Cell::new(r, c)).unwrap(),
                    expected,
                    "cell ({r},{c}) should be pruned to {{1,2}}"
                );
            }
        }
    }

    // --- Puzzle::solutions ---

    // A 2×2 puzzle with no cages has two valid latin squares:
    //   [[1,2],[2,1]] and [[2,1],[1,2]].
    #[test]
    fn solutions_no_cages_yields_all_latin_squares() {
        let p = Puzzle::new(2).unwrap();
        let solutions: Vec<Puzzle> = p.solutions().map(Result::unwrap).collect();
        assert_eq!(solutions.len(), 2);
        for sol in &solutions {
            for r in 0..2 {
                for c in 0..2 {
                    assert!(sol.get_cell_values(Cell::new(r, c)).unwrap().is_singleton());
                }
            }
        }
    }

    // The solved_2x2 puzzle (all Given cages) already has a unique solution;
    // solutions() should yield exactly that one.
    #[test]
    fn solutions_fully_caged_yields_one_solution() {
        let solutions: Vec<Puzzle> = solved_2x2().solutions().map(Result::unwrap).collect();
        assert_eq!(solutions.len(), 1);
        let sol = &solutions[0];
        assert_eq!(
            sol.get_cell_values(Cell::new(0, 0)).unwrap(),
            Values::new(&[1])
        );
        assert_eq!(
            sol.get_cell_values(Cell::new(0, 1)).unwrap(),
            Values::new(&[2])
        );
        assert_eq!(
            sol.get_cell_values(Cell::new(1, 0)).unwrap(),
            Values::new(&[2])
        );
        assert_eq!(
            sol.get_cell_values(Cell::new(1, 1)).unwrap(),
            Values::new(&[1])
        );
    }

    // A 2×2 puzzle with an impossible constraint (Add target=0 is unreachable) yields no solutions.
    #[test]
    fn solutions_infeasible_yields_none() {
        // Given cage with value 5 is out of range for a 2×2 (valid values: 1..=2).
        let p = Puzzle::new(2)
            .unwrap()
            .insert_cage(cage_at(&[(0, 0)], Operator::Given, 5));
        assert!(p.solutions().map(Result::unwrap).next().is_none());
    }

    // A mixed-arithmetic 2×2:
    //   (0,0)+(0,1) = 3  →  only {(1,2),(2,1)}
    //   (1,0) given = 2, (1,1) given = 1
    // The unique solution is [[1,2],[2,1]].
    #[test]
    fn solutions_mixed_cages_unique_solution() {
        let p = Puzzle::new(2)
            .unwrap()
            .insert_cage(cage_at(&[(0, 0), (0, 1)], Operator::Add, 3))
            .insert_cage(cage_at(&[(1, 0)], Operator::Given, 2))
            .insert_cage(cage_at(&[(1, 1)], Operator::Given, 1));
        let solutions: Vec<Puzzle> = p.solutions().map(Result::unwrap).collect();
        assert_eq!(solutions.len(), 1);
        let sol = &solutions[0];
        assert_eq!(
            sol.get_cell_values(Cell::new(0, 0)).unwrap(),
            Values::new(&[1])
        );
        assert_eq!(
            sol.get_cell_values(Cell::new(0, 1)).unwrap(),
            Values::new(&[2])
        );
        assert_eq!(
            sol.get_cell_values(Cell::new(1, 0)).unwrap(),
            Values::new(&[2])
        );
        assert_eq!(
            sol.get_cell_values(Cell::new(1, 1)).unwrap(),
            Values::new(&[1])
        );
    }

    // A 3×3 puzzle with three row-sum cages (each row sums to 6).
    // There are multiple valid latin squares satisfying this, so we check
    // solution quality rather than count: every solution must be fully
    // assigned and each row must sum to 6.
    #[test]
    fn solutions_3x3_row_sum_cages_all_valid() {
        let p = Puzzle::new(3)
            .unwrap()
            .insert_cage(cage_at(&[(0, 0), (0, 1), (0, 2)], Operator::Add, 6))
            .insert_cage(cage_at(&[(1, 0), (1, 1), (1, 2)], Operator::Add, 6))
            .insert_cage(cage_at(&[(2, 0), (2, 1), (2, 2)], Operator::Add, 6));
        let solutions: Vec<Puzzle> = p.solutions().map(Result::unwrap).collect();
        assert!(!solutions.is_empty(), "should have at least one solution");
        for sol in &solutions {
            for r in 0..3 {
                // Every cell is a singleton.
                for c in 0..3 {
                    assert!(
                        sol.get_cell_values(Cell::new(r, c)).unwrap().is_singleton(),
                        "cell ({r},{c}) should be singleton in every solution"
                    );
                }
                // Each row sums to 6.
                let row_sum: u32 = (0..3)
                    .map(|c| u32::from(sol.get_cell_values(Cell::new(r, c)).unwrap().values()[0]))
                    .sum();
                assert_eq!(row_sum, 6, "row {r} should sum to 6");
            }
        }
    }

    #[test]
    fn insert_cage_accumulates_cages() {
        let p = Puzzle::new(4).unwrap();
        let c1 = cage_at(&[(0, 0)], Operator::Given, 1);
        let c2 = cage_at(&[(0, 1)], Operator::Given, 2);
        let p3 = p.insert_cage(c1).insert_cage(c2);
        // Both cages present — domains still accessible.
        assert!(p3.get_cell_values(Cell::new(0, 0)).is_ok());
        assert!(p3.get_cell_values(Cell::new(0, 1)).is_ok());
    }
}
