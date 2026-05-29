//! Core puzzle generation: assigns operations and targets to cages over a
//! solved Latin square.
//!
//! The public entry points are [`generate`] (default settings) and
//! [`generate_with`] (custom operation policy and cage-size distribution).

#![allow(clippy::cast_precision_loss)] // usize→f64 for Poisson mean/sample; values are small

use std::collections::HashSet;

use rand::{Rng, RngExt};

use crate::Cell;
use crate::Error;
use crate::cage::Cage;
use crate::cell::{Target, Value};
use crate::latin_square::generate_latin_square;
use crate::operation::{Operation, Operator};
use crate::polyomino::Polyomino;
use crate::puzzle::Puzzle;
/// A Poisson cage-size distribution truncated to `[1, n²]` by rejection sampling.
///
/// Cage sizes are drawn from `Poisson(mean)` and resampled until the result
/// falls in `[1, n²]`. The mean must be strictly positive so rejection sampling
/// is guaranteed to terminate.
#[derive(Debug, Clone, Copy)]
pub struct SizeDistribution {
    mean: f64,
}

impl SizeDistribution {
    /// Default distribution for an `n`×`n` grid: `Poisson(n / 3)`.
    ///
    /// For `n = 0`, the same distribution is returned as for `n = 1`. The
    /// puzzle constructor rejects `n = 0` independently, so the degenerate
    /// case never propagates to sampling.
    pub fn default_for(n: usize) -> Self {
        Self {
            mean: n.max(1) as f64 / 3.0,
        }
    }

    /// Samples a cage size in `[1, n*n]` by rejection sampling on
    /// `Poisson(mean)`.
    fn sample<R: Rng>(self, n: usize, rng: &mut R) -> usize {
        let max = n * n;
        loop {
            let k = poisson(self.mean, rng);
            if (1..=max).contains(&k) {
                return k;
            }
        }
    }
}

/// Draws a sample from `Poisson(mean)` using Knuth's algorithm. Adequate for
/// the small means used here (mean ≤ 3 for n ≤ 9).
fn poisson<R: Rng>(mean: f64, rng: &mut R) -> usize {
    let l = (-mean).exp();
    let mut k = 0usize;
    let mut p = 1.0f64;
    loop {
        k += 1;
        p *= rng.random::<f64>();
        if p <= l {
            return k - 1;
        }
    }
}

/// Default policy mapping a cage's solved-grid values to an [`Operation`].
///
/// - 1 cell: [`Operator::Given`].
/// - 2 cells: [`Operator::Divide`] when divisible, otherwise [`Operator::Subtract`].
/// - 3+ cells: [`Operator::Multiply`] when the product fits in `n²`, otherwise [`Operator::Add`].
///
/// # Errors
/// Returns [`Error::EmptyOpPolicyValues`] if `values` is empty. A cage always
/// covers at least one cell, so callers that obtain `values` from a cage's
/// cells will never trigger this.
pub fn default_op_policy(values: &[Value], n: usize) -> Result<Operation, Error> {
    let op = |operator, target| Ok(Operation::new(operator, target));
    match values.len() {
        0 => Err(Error::EmptyOpPolicyValues),
        1 => op(Operator::Given, Target::from(values[0])),
        2 => {
            let (hi, lo) = (values[0].max(values[1]), values[0].min(values[1]));
            if hi.is_multiple_of(lo) {
                op(Operator::Divide, Target::from(hi / lo))
            } else {
                op(Operator::Subtract, Target::from(hi - lo))
            }
        }
        _ => {
            let prod: Target = values.iter().map(|&v| Target::from(v)).product();
            let area = Target::try_from(n * n).unwrap_or(Target::MAX);
            if prod <= area {
                op(Operator::Multiply, prod)
            } else {
                op(Operator::Add, values.iter().map(|&v| Target::from(v)).sum())
            }
        }
    }
}

/// Generates a random `n`×`n` puzzle using the default operation policy and
/// a default Poisson size distribution.
///
/// # Errors
/// Returns `Error` if `n` is not in `1..=9`.
pub fn generate<R: Rng>(n: usize, rng: &mut R) -> Result<Puzzle, Error> {
    generate_with(n, rng, default_op_policy, SizeDistribution::default_for(n))
}

/// Generates a random `n`×`n` puzzle with caller-supplied op policy and
/// cage-size distribution.
///
/// The pipeline is:
/// 1. Sample a uniformly random Latin square as the puzzle's solution.
/// 2. Tile the grid with random polyominos sized by `sizes`.
/// 3. For each polyomino, look up the Latin-square values at its cells (in row-major sorted order)
///    and pass them to `op` to choose the cage's operation.
///
/// # Errors
/// Returns `Error` if `n` is not in `1..=9`, or any error returned by `op`.
#[allow(clippy::cast_possible_truncation)]
pub fn generate_with<R: Rng, F>(
    n: usize,
    rng: &mut R,
    op: F,
    sizes: SizeDistribution,
) -> Result<Puzzle, Error>
where
    F: Fn(&[Value], usize) -> Result<Operation, Error>,
{
    let mut puzzle = Puzzle::new(n)?;
    let latin_square = generate_latin_square(n, rng);
    let tiling = greedy(n, sizes, rng)?;

    for polyomino in tiling {
        let values: Vec<Value> = polyomino
            .cells()
            .into_iter()
            .map(|cell| latin_square[cell.row][cell.column])
            .collect();
        let operation = op(&values, n)?;
        let cage = Cage::new(polyomino, operation)?;
        puzzle = puzzle.insert_cage(cage)?;
    }
    Ok(puzzle)
}

/// Builds a tiling that fully covers an `n`×`n` grid by greedy growth.
///
/// Repeatedly seeds a random uncovered cell, grows it by absorbing random
/// edge-connected uncovered cells until the target size sampled from
/// `dist` is reached or no candidates remain, then starts a new
/// polyomino.
///
/// # Errors
/// Returns [`Error::DisconnectedPolyomino`] or [`Error::EmptyPolyomino`] if
/// the grown cell set fails validation (structurally unreachable in practice).
pub fn greedy<R: Rng>(
    n: usize,
    dist: SizeDistribution,
    rng: &mut R,
) -> Result<Vec<Polyomino>, Error> {
    let mut tiling = Vec::new();
    let mut covered: HashSet<Cell> = HashSet::with_capacity(n * n);

    while covered.len() < n * n {
        let uncovered: Vec<Cell> = (0..n)
            .flat_map(|r| (0..n).map(move |c| Cell::new(r, c)))
            .filter(|c| !covered.contains(c))
            .collect();
        let seed = uncovered[rng.random_range(0..uncovered.len())];
        let target_size = dist.sample(n, rng);

        let mut cells: HashSet<Cell> = HashSet::new();
        let _ = cells.insert(seed);
        // Frontier may contain duplicates; dedup happens on pop via the cells/covered
        // checks.
        let mut frontier: Vec<Cell> = grid_neighbors(seed, n)
            .filter(|c| !covered.contains(c))
            .collect();

        while cells.len() < target_size && !frontier.is_empty() {
            let pick_idx = rng.random_range(0..frontier.len());
            let pick = frontier.swap_remove(pick_idx);
            if !cells.insert(pick) {
                continue;
            }
            for neighbor in grid_neighbors(pick, n) {
                if !covered.contains(&neighbor) && !cells.contains(&neighbor) {
                    frontier.push(neighbor);
                }
            }
        }

        for c in &cells {
            let _ = covered.insert(*c);
        }
        let cells: Vec<Cell> = cells.into_iter().collect();
        // `cells` is non-empty (the seed is always inserted) and
        // edge-connected (grown only via `grid_neighbors`), so the
        // validating `Polyomino::new` would always succeed here.
        tiling.push(Polyomino::from_cells(&cells)?);
    }

    Ok(tiling)
}

/// In-bounds 4-neighbors of `cell` in an `n`×`n` grid.
fn grid_neighbors(cell: Cell, n: usize) -> impl Iterator<Item = Cell> {
    cell.neighbors_4()
        .filter(move |c| c.row < n && c.column < n)
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    use super::*;

    impl SizeDistribution {
        fn new(mean: f64) -> Option<Self> {
            (mean > 0.0).then_some(Self { mean })
        }

        const fn mean(self) -> f64 {
            self.mean
        }
    }

    fn op(operator: Operator, target: Target) -> Operation {
        Operation::new(operator, target)
    }

    #[test]
    fn default_op_policy_one_cell_is_given() {
        assert_eq!(default_op_policy(&[3], 4).unwrap(), op(Operator::Given, 3));
    }

    #[test]
    fn default_op_policy_two_cells_divisible_is_divide() {
        assert_eq!(
            default_op_policy(&[2, 6], 6).unwrap(),
            op(Operator::Divide, 3)
        );
    }

    #[test]
    fn default_op_policy_two_cells_not_divisible_is_subtract() {
        assert_eq!(
            default_op_policy(&[2, 5], 6).unwrap(),
            op(Operator::Subtract, 3)
        );
    }

    #[test]
    fn default_op_policy_three_cells_product_within_n_squared_is_multiply() {
        // n²=16; product 1·2·3 = 6 ≤ 16
        assert_eq!(
            default_op_policy(&[1, 2, 3], 4).unwrap(),
            op(Operator::Multiply, 6)
        );
    }

    #[test]
    fn default_op_policy_three_cells_product_above_n_squared_is_add() {
        // n²=16; product 3·4·4 = 48 > 16
        assert_eq!(
            default_op_policy(&[3, 4, 4], 4).unwrap(),
            op(Operator::Add, 11)
        );
    }

    #[test]
    fn default_op_policy_overflowing_product_falls_back_to_add() {
        // 9^9 = 387_420_489 overflows M (u16); must fall back to Add.
        // sum = 9*9 = 81
        assert_eq!(
            default_op_policy(&[9, 9, 9, 9, 9, 9, 9, 9, 9], 9).unwrap(),
            op(Operator::Add, 81)
        );
    }

    #[test]
    fn default_op_policy_empty_returns_err() {
        assert!(matches!(
            default_op_policy(&[], 4),
            Err(Error::EmptyOpPolicyValues)
        ));
    }

    #[test]
    fn size_distribution_poisson_samples_within_bounds() {
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        for (mean, n) in [(0.5_f64, 3_usize), (1.0, 4), (3.0, 9), (5.0, 4)] {
            let dist = SizeDistribution::new(mean).unwrap();
            for _ in 0..200 {
                let s = dist.sample(n, &mut rng);
                assert!((1..=n * n).contains(&s));
            }
        }
    }

    #[test]
    fn size_distribution_new_rejects_non_positive_mean() {
        assert!(SizeDistribution::new(0.0).is_none());
        assert!(SizeDistribution::new(-1.0).is_none());
        assert!(SizeDistribution::new(f64::NAN).is_none());
        assert!(SizeDistribution::new(0.5).is_some());
    }

    #[test]
    fn size_distribution_high_rejection_terminates_and_stays_in_bounds() {
        // Mean (8) is well above the upper bound (n*n = 4), so most raw
        // Poisson draws are rejected. Sampling must still terminate and
        // never escape the truncation window.
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let dist = SizeDistribution::new(8.0).unwrap();
        for _ in 0..100 {
            let s = dist.sample(2, &mut rng);
            assert!((1..=4).contains(&s));
        }
    }

    #[test]
    fn size_distribution_default_for_uses_n_over_three() {
        assert!((SizeDistribution::default_for(9).mean() - 3.0).abs() < 1e-12);
        assert!((SizeDistribution::default_for(3).mean() - 1.0).abs() < 1e-12);
        assert!((SizeDistribution::default_for(4).mean() - 4.0 / 3.0).abs() < 1e-12);
        // n = 0 clamps to n = 1.
        assert!((SizeDistribution::default_for(0).mean() - 1.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn poisson_empirical_mean_close_to_target() {
        let mut rng = ChaCha8Rng::seed_from_u64(123);
        for target in [0.5_f64, 1.0, 2.5, 4.0] {
            let n_samples: usize = 20_000;
            let sum: usize = (0..n_samples).map(|_| poisson(target, &mut rng)).sum();
            let empirical = sum as f64 / n_samples as f64;
            assert!(
                (empirical - target).abs() < 0.1,
                "empirical mean {empirical} too far from target {target}"
            );
        }
    }

    #[test]
    fn greedy_covers_all_cells() {
        // Run many seeds across different means to maximize branch coverage.
        for seed in 0u64..200 {
            let mean = if seed % 2 == 0 { 1.0 } else { 2.5 };
            let dist = SizeDistribution::new(mean).unwrap();
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let n = if seed % 3 == 0 { 4 } else { 3 };
            let tiling = greedy(n, dist, &mut rng).unwrap();
            let covered: HashSet<Cell> = tiling.iter().flat_map(Polyomino::cells).collect();
            assert_eq!(covered.len(), n * n);
        }
    }

    #[test]
    fn generate_returns_a_puzzle() {
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        assert!(generate(4, &mut rng).is_ok());
    }

    #[test]
    fn generate_invalid_n_returns_err() {
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        assert!(generate(0, &mut rng).is_err());
        assert!(generate(10, &mut rng).is_err());
    }
}
