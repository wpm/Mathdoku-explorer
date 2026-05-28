//! Global-feasibility queries for Without-Solution authoring.
//!
//! In Without-Solution mode every operator and target the author picks must
//! leave at least one global completion of the puzzle. These queries reuse the
//! existing CSP/MDD solver ([`Grid::solutions`]) rather than introducing a
//! parallel engine, with an early exit at the first completion.
//!
//! [`feasible_op_targets`] enumerates the globally-feasible `(operator, target)`
//! pairs for a candidate cage; [`cached_feasible_op_targets`] memoizes that
//! result so the dropdown does not recompute on every open.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

use mathdoku::{Cage, Cell, Grid, Operation, Operator, Polyomino, Puzzle, Target, operators};

/// Products above this ceiling are never offered as `Multiply` targets. No
/// realistic cage in an `n ≤ 9` grid has a larger product, and the bound keeps
/// the candidate enumeration finite for pathologically large cages.
const MAX_PRODUCT: Target = 1_000_000_000;

/// Returns `true` if the puzzle extended with `candidate` has at least one
/// global completion.
///
/// The query constrains a fresh grid by the extended puzzle and asks the
/// solution iterator for its first completion, stopping immediately once one
/// is found. A cage conflict, a propagation wipeout, or an empty solution
/// stream all mean infeasible.
#[must_use]
pub fn is_globally_feasible(puzzle: &Puzzle, candidate: &Cage) -> bool {
    let Ok(extended) = puzzle.insert_cage(candidate.clone()) else {
        return false;
    };
    let Ok(initial) = Grid::new(puzzle.n()).and_then(|g| g.constrain(&extended)) else {
        return false;
    };
    matches!(initial.solutions(&extended).next(), Some(Ok(_)))
}

/// Enumerates all globally-feasible `(operator, target)` pairs for `polyomino`
/// against the current `puzzle`.
///
/// Per-pair strategy (see issue #25): for each operator valid for the cage's
/// size, every candidate target is tested with [`is_globally_feasible`]. The
/// candidate targets are a tight superset derived by reachability, so the only
/// pairs returned are the ones that actually admit a completion.
#[must_use]
pub fn feasible_op_targets(puzzle: &Puzzle, polyomino: &Polyomino) -> Vec<(Operator, Target)> {
    let n = puzzle.n();
    let k = polyomino.len();
    let mut out = Vec::new();
    for op in operators(polyomino) {
        for target in candidate_targets(&op, k, n) {
            let cage = Cage::new(polyomino.clone(), Operation::new(op.clone(), target));
            if is_globally_feasible(puzzle, &cage) {
                out.push((op.clone(), target));
            }
        }
    }
    out
}

/// Candidate targets to test for `op` on a `k`-cell cage in an `n×n` grid.
///
/// This is a *superset* of the achievable targets; global feasibility is the
/// final filter. Sums and products are enumerated by reachability so the ranges
/// stay tight without iterating all integers up to `n^k`.
fn candidate_targets(op: &Operator, k: usize, n: usize) -> Vec<Target> {
    let n = n as Target;
    match op {
        Operator::Given => (1..=n).collect(),
        // A 2-cell collinear pair can never differ by 0 or by `n` or more.
        Operator::Subtract => (1..n).collect(),
        // A ratio of 1 is impossible for distinct collinear cells; the largest
        // possible ratio is `n` (e.g. `n` over `1`).
        Operator::Divide => (2..=n).collect(),
        Operator::Add => reachable(k, n, 0, |acc, v| acc + v)
            .into_iter()
            .filter(|&t| t > 0)
            .collect(),
        Operator::Multiply => reachable(k, n, 1, Target::saturating_mul)
            .into_iter()
            .collect(),
    }
}

/// Reachable accumulator values after combining `k` cells, each contributing a
/// value in `1..=n` via `step`, starting from `seed`. Values exceeding
/// [`MAX_PRODUCT`] are pruned to keep the set finite.
fn reachable(
    k: usize,
    n: Target,
    seed: Target,
    step: impl Fn(Target, Target) -> Target,
) -> BTreeSet<Target> {
    let mut acc: BTreeSet<Target> = BTreeSet::from([seed]);
    for _ in 0..k {
        let mut next = BTreeSet::new();
        for &a in &acc {
            for v in 1..=n {
                let r = step(a, v);
                if r <= MAX_PRODUCT {
                    let _ = next.insert(r);
                }
            }
        }
        acc = next;
        if acc.is_empty() {
            break;
        }
    }
    acc
}

// ---- dropdown-query cache (Piece 5) ----
//
// Keyed on `(puzzle content hash, cage cells)`. Using a content hash of the
// committed cages rather than a manual version counter makes the cache
// self-invalidating: any commit, delete, fix, or unfix changes the puzzle and
// therefore the key, so there is no "forgot to bump the counter" staleness bug.
// WASM is single-threaded, so a `thread_local` is sound and needs no locking.

/// Cache key: a content hash of the committed cages, plus the candidate cage's cells.
type CacheKey = (u64, Vec<Cell>);
type FeasibleCache = HashMap<CacheKey, Vec<(Operator, Target)>>;

thread_local! {
    static CACHE: RefCell<FeasibleCache> = RefCell::new(FeasibleCache::new());
}

fn puzzle_key(puzzle: &Puzzle) -> u64 {
    let mut hasher = DefaultHasher::new();
    puzzle.hash(&mut hasher);
    hasher.finish()
}

/// Memoized [`feasible_op_targets`]. Returns the cached result for the
/// `(puzzle, polyomino)` pair if present, otherwise computes and stores it.
#[must_use]
pub fn cached_feasible_op_targets(
    puzzle: &Puzzle,
    polyomino: &Polyomino,
) -> Vec<(Operator, Target)> {
    let key = (puzzle_key(puzzle), polyomino.cells());
    if let Some(hit) = CACHE.with_borrow(|c| c.get(&key).cloned()) {
        return hit;
    }
    let result = feasible_op_targets(puzzle, polyomino);
    CACHE.with_borrow_mut(|c| {
        let _ = c.insert(key, result.clone());
    });
    result
}

/// Groups feasible `(operator, target)` pairs by operator.
///
/// The operator order of [`operators`] is preserved. Used by the
/// Without-Solution two-step picker: the operator strip shows the keys, and
/// clicking one reveals its targets.
#[must_use]
pub fn group_by_operator(pairs: &[(Operator, Target)]) -> Vec<(Operator, Vec<Target>)> {
    let mut grouped: Vec<(Operator, Vec<Target>)> = Vec::new();
    for (op, target) in pairs {
        if let Some(entry) = grouped.iter_mut().find(|(o, _)| o == op) {
            entry.1.push(*target);
        } else {
            grouped.push((op.clone(), vec![*target]));
        }
    }
    grouped
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{
        cached_feasible_op_targets, candidate_targets, feasible_op_targets, group_by_operator,
        is_globally_feasible,
    };
    use mathdoku::{Cage, Cell, Operation, Operator, Polyomino, Puzzle};

    fn poly(positions: &[(usize, usize)]) -> Polyomino {
        let cells: Vec<Cell> = positions.iter().map(|&(r, c)| Cell::new(r, c)).collect();
        Polyomino::from_cells(&cells).unwrap()
    }

    fn cage(positions: &[(usize, usize)], op: Operator, target: u64) -> Cage {
        Cage::new(poly(positions), Operation::new(op, target))
    }

    #[test]
    fn given_in_range_is_feasible_in_empty_puzzle() {
        let puzzle = Puzzle::new(3).unwrap();
        assert!(is_globally_feasible(
            &puzzle,
            &cage(&[(0, 0)], Operator::Given, 2)
        ));
    }

    #[test]
    fn given_out_of_range_is_infeasible() {
        let puzzle = Puzzle::new(3).unwrap();
        assert!(!is_globally_feasible(
            &puzzle,
            &cage(&[(0, 0)], Operator::Given, 9)
        ));
    }

    #[test]
    fn overlapping_cage_is_infeasible() {
        // A cage conflict (overlapping an existing cage) is never feasible.
        let puzzle = Puzzle::new(3)
            .unwrap()
            .insert_cage(cage(&[(0, 0), (0, 1)], Operator::Add, 3))
            .unwrap();
        assert!(!is_globally_feasible(
            &puzzle,
            &cage(&[(0, 0)], Operator::Given, 1)
        ));
    }

    #[test]
    fn given_singleton_offers_every_value_in_an_empty_grid() {
        let puzzle = Puzzle::new(4).unwrap();
        let pairs = feasible_op_targets(&puzzle, &poly(&[(1, 1)]));
        // A single cell in an empty 4×4 can hold any of 1..=4.
        let targets: Vec<u64> = pairs.iter().map(|&(_, t)| t).collect();
        assert_eq!(targets, vec![1, 2, 3, 4]);
        assert!(pairs.iter().all(|(op, _)| *op == Operator::Given));
    }

    #[test]
    fn row_triple_in_3x3_only_admits_sum_and_product_six() {
        // A full row of a 3×3 must be a permutation of {1,2,3}: sum 6, product 6.
        let puzzle = Puzzle::new(3).unwrap();
        let pairs = feasible_op_targets(&puzzle, &poly(&[(0, 0), (0, 1), (0, 2)]));
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
        for (op, target) in feasible_op_targets(&puzzle, &p) {
            assert!(is_globally_feasible(
                &puzzle,
                &Cage::new(p.clone(), Operation::new(op, target))
            ));
        }
    }

    #[test]
    fn candidate_targets_add_pair_is_bounded_by_two_n() {
        let targets = candidate_targets(&Operator::Add, 2, 4);
        // Two cells in 1..=4 sum to between 2 and 8.
        assert_eq!(*targets.first().unwrap(), 2);
        assert_eq!(*targets.last().unwrap(), 8);
    }

    #[test]
    fn cache_returns_same_result_as_direct_call() {
        let puzzle = Puzzle::new(4).unwrap();
        let p = poly(&[(2, 2), (2, 3)]);
        let direct = feasible_op_targets(&puzzle, &p);
        let cached_once = cached_feasible_op_targets(&puzzle, &p);
        let cached_twice = cached_feasible_op_targets(&puzzle, &p);
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
}
