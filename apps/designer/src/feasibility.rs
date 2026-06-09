//! Global-feasibility queries for Without-Solution authoring.
//!
//! In Without-Solution mode every operator and target the author picks must
//! leave at least one global completion of the puzzle. These queries reuse the
//! existing CSP/MDD solver ([`Puzzle::solutions`]) rather than introducing a
//! parallel engine, with an early exit at the first completion.
//!
//! [`feasible_op_targets`] enumerates the globally-feasible `(operator, target)`
//! pairs for a candidate cage; [`cached_feasible_op_targets`] memoizes that
//! result so the dropdown does not recompute on every open.

use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use mathdoku::{Cage, Error, N, Operator, Polyomino, Puzzle, Target};

/// Returns `true` if the puzzle extended with `candidate` has at least one
/// global completion.
///
/// The query constrains a fresh grid by the extended puzzle and asks the
/// solution iterator for its first completion, stopping immediately once one
/// is found. A cage conflict, a propagation wipeout, or an empty solution
/// stream all mean infeasible.
#[must_use]
pub fn is_globally_feasible(puzzle: &Puzzle, candidate: &Cage) -> bool {
    let Ok(Some(extended)) = puzzle.insert_cage(candidate) else {
        return false;
    };
    matches!(extended.solutions().next(), Some(Ok(_)))
}

/// Enumerates all globally-feasible `(operator, target)` pairs for `polyomino`
/// against the current `puzzle`.
///
/// For each locally-feasible operator and target (from [`Puzzle::possible_operations`]
/// and [`Puzzle::possible_targets`]), tests global feasibility with
/// [`is_globally_feasible`]. Only pairs that admit a completion are returned.
///
/// # Errors
/// Returns [`Error`] if constructing a candidate cage fails.
pub fn feasible_op_targets(
    puzzle: &Puzzle,
    polyomino: &Polyomino,
) -> Result<Vec<(Operator, Target)>, Error> {
    let n = N::try_from(puzzle.n()).map_err(|_| Error::InvalidGridSize(puzzle.n()))?;
    let mut out = Vec::new();
    for op in puzzle.possible_operations(polyomino)? {
        for target in puzzle.possible_targets(polyomino, op)? {
            let cage = Cage::new(n, polyomino.clone(), op, target)?;
            if is_globally_feasible(puzzle, &cage) {
                out.push((op, target));
            }
        }
    }
    Ok(out)
}

// ---- dropdown-query cache (Piece 5) ----
//
// Keyed on `(puzzle content hash, cage cells)`. Using a content hash of the
// committed cages rather than a manual version counter makes the cache
// self-invalidating: any commit, delete, fix, or unfix changes the puzzle and
// therefore the key, so there is no "forgot to bump the counter" staleness bug.
// WASM is single-threaded, so a `thread_local` is sound and needs no locking.

/// Cache key: a content hash of the committed cages, plus the candidate polyomino.
type CacheKey = (u64, Polyomino);
type FeasibleCache = HashMap<CacheKey, Vec<(Operator, Target)>>;

thread_local! {
    static CACHE: RefCell<FeasibleCache> = RefCell::new(FeasibleCache::new());
}

/// Produces a compact cache key for `puzzle`.
///
/// `Puzzle` is too large to store as a `HashMap` key directly, and its
/// `PartialEq` only covers `n` and `cages` while its `Hash` also covers the
/// internal grid — making it unsuitable as a key type. A `u64` content hash
/// is cheap to compute and small to store.
fn puzzle_key(puzzle: &Puzzle) -> u64 {
    let mut hasher = DefaultHasher::new();
    puzzle.hash(&mut hasher);
    hasher.finish()
}

/// Memoized [`feasible_op_targets`]. Returns the cached result for the
/// `(puzzle, polyomino)` pair if present, otherwise computes and stores it.
///
/// # Errors
/// Returns [`Error`] if constructing a candidate cage fails.
pub fn cached_feasible_op_targets(
    puzzle: &Puzzle,
    polyomino: &Polyomino,
) -> Result<Vec<(Operator, Target)>, Error> {
    let key = (puzzle_key(puzzle), polyomino.clone());
    if let Some(hit) = CACHE.with_borrow(|c| c.get(&key).cloned()) {
        return Ok(hit);
    }
    let result = feasible_op_targets(puzzle, polyomino)?;
    CACHE.with_borrow_mut(|c| {
        let _ = c.insert(key, result.clone());
    });
    Ok(result)
}

/// Groups feasible `(operator, target)` pairs by operator.
///
/// The operator order from [`Puzzle::possible_operations`] is preserved. Used by the
/// Without-Solution two-step picker: the operator strip shows the keys, and
/// clicking one reveals its targets.
#[must_use]
pub fn group_by_operator(pairs: &[(Operator, Target)]) -> Vec<(Operator, Vec<Target>)> {
    let mut grouped: Vec<(Operator, Vec<Target>)> = Vec::new();
    for (op, target) in pairs {
        if let Some(entry) = grouped.iter_mut().find(|(o, _)| o == op) {
            entry.1.push(*target);
        } else {
            grouped.push((*op, vec![*target]));
        }
    }
    grouped
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{
        cached_feasible_op_targets, feasible_op_targets, group_by_operator, is_globally_feasible,
    };
    use mathdoku::{Cage, Cell, N, Operator, Polyomino, Puzzle};

    fn poly(positions: &[(usize, usize)]) -> Polyomino {
        let cells: Vec<Cell> = positions.iter().map(|&(r, c)| Cell::new(r, c)).collect();
        Polyomino::from_cells(&cells).unwrap()
    }

    fn cage(n: N, positions: &[(usize, usize)], op: Operator, target: mathdoku::T) -> Cage {
        Cage::new(n, poly(positions), op, target).unwrap()
    }

    #[test]
    fn given_in_range_is_feasible_in_empty_puzzle() {
        let puzzle = Puzzle::new(3).unwrap();
        assert!(is_globally_feasible(
            &puzzle,
            &cage(3, &[(0, 0)], Operator::Given, 2)
        ));
    }

    #[test]
    fn given_out_of_range_is_infeasible() {
        let puzzle = Puzzle::new(3).unwrap();
        assert!(!is_globally_feasible(
            &puzzle,
            &cage(3, &[(0, 0)], Operator::Given, 9)
        ));
    }

    #[test]
    fn overlapping_cage_is_infeasible() {
        // A cage conflict (overlapping an existing cage) is never feasible.
        let puzzle = Puzzle::new(3)
            .unwrap()
            .insert_cage(&cage(3, &[(0, 0), (0, 1)], Operator::Add, 3))
            .unwrap()
            .unwrap();
        assert!(!is_globally_feasible(
            &puzzle,
            &cage(3, &[(0, 0)], Operator::Given, 1)
        ));
    }

    #[test]
    fn given_singleton_offers_every_value_in_an_empty_grid() {
        let puzzle = Puzzle::new(4).unwrap();
        let pairs = feasible_op_targets(&puzzle, &poly(&[(1, 1)])).unwrap();
        // A single cell in an empty 4×4 can hold any of 1..=4.
        let targets: Vec<mathdoku::Target> = pairs.iter().map(|&(_, t)| t).collect();
        assert_eq!(targets, vec![1, 2, 3, 4]);
        assert!(pairs.iter().all(|(op, _)| *op == Operator::Given));
    }

    #[test]
    fn row_triple_in_3x3_only_admits_sum_and_product_six() {
        // A full row of a 3×3 must be a permutation of {1,2,3}: sum 6, product 6.
        let puzzle = Puzzle::new(3).unwrap();
        let pairs = feasible_op_targets(&puzzle, &poly(&[(0, 0), (0, 1), (0, 2)])).unwrap();
        assert!(pairs.contains(&(Operator::Add, 6)));
        assert!(pairs.contains(&(Operator::Multiply, 6)));
        // No other Add/Multiply targets are reachable.
        for (op, target) in &pairs {
            match op {
                Operator::Add | Operator::Multiply => assert_eq!(*target, 6),
                other => panic!("unexpected operator {other:?} for a triple"),
            }
        }
    }

    #[test]
    fn every_returned_pair_is_individually_feasible() {
        let puzzle = Puzzle::new(4).unwrap();
        let p = poly(&[(0, 0), (0, 1)]);
        for (op, target) in feasible_op_targets(&puzzle, &p).unwrap() {
            assert!(is_globally_feasible(
                &puzzle,
                &Cage::new(N::try_from(puzzle.n()).unwrap(), p.clone(), op, target).unwrap()
            ));
        }
    }

    #[test]
    fn cache_returns_same_result_as_direct_call() {
        let puzzle = Puzzle::new(4).unwrap();
        let p = poly(&[(2, 2), (2, 3)]);
        let direct = feasible_op_targets(&puzzle, &p).unwrap();
        let cached_once = cached_feasible_op_targets(&puzzle, &p).unwrap();
        let cached_twice = cached_feasible_op_targets(&puzzle, &p).unwrap();
        assert_eq!(direct, cached_once);
        assert_eq!(cached_once, cached_twice);
    }

    #[test]
    fn group_by_operator_preserves_operator_order_and_collects_targets() {
        let pairs = vec![
            (Operator::Add, 3),
            (Operator::Add, 4),
            (Operator::Subtract, 1),
        ];
        let grouped = group_by_operator(&pairs);
        assert_eq!(grouped[0].0, Operator::Add);
        assert_eq!(grouped[0].1, vec![3, 4]);
        assert_eq!(grouped[1].0, Operator::Subtract);
        assert_eq!(grouped[1].1, vec![1]);
    }

    /// Perf baseline: `feasible_op_targets` for a 2×2 square polyomino in an
    /// empty 7×7 Without-Solution puzzle. This mirrors what happens when the
    /// user draws a 2×2 cage and presses Enter in the designer — the operation
    /// selector triggers this call to populate its operator/target picker.
    #[test]
    fn perf_feasible_op_targets_2x2_in_empty_7x7() {
        let puzzle = Puzzle::new(7).unwrap();
        let square = poly(&[(0, 0), (0, 1), (1, 0), (1, 1)]);
        let pairs = feasible_op_targets(&puzzle, &square).unwrap();
        assert!(!pairs.is_empty());
    }
}
