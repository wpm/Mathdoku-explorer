//! Core puzzle generation: assigns operations and targets to cages over a
//! solved Latin square.

#![allow(clippy::cast_precision_loss)] // usize→f64 for Poisson mean/sample; values are small

use std::collections::HashSet;

use rand::{Rng, RngExt};

use crate::latin_square::generate_latin_square;
use crate::polyomino::{Cell, Polyomino};
use crate::puzzle::{CageOperator, Puzzle};
use crate::{Error, N, T};

/// A Poisson cage-size distribution truncated to `[1, n²]` by rejection sampling.
///
/// Cage sizes are drawn from `Poisson(mean)` and resampled until the result
/// falls in `[1, n²]`. The mean must be strictly positive so rejection sampling
/// is guaranteed to terminate.
#[must_use]
#[derive(Debug, Clone, Copy)]
pub struct SizeDistribution {
    mean: f64,
}

impl SizeDistribution {
    /// Default distribution for an `n`×`n` grid: `Poisson(n / 3)`.
    pub fn default_for(n: usize) -> Self {
        Self {
            mean: n.max(1) as f64 / 3.0,
        }
    }

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

/// Default policy mapping a cage's solved-grid values to a `(CageOperator, T)` pair.
///
/// - 1 cell: `Given`.
/// - 2 cells: `Divide` when divisible, otherwise `Subtract`.
/// - 3+ cells: `Multiply` when the product fits in `n²`, otherwise `Add`.
///
/// # Errors
/// Returns [`Error::EmptyFills`] if `values` is empty.
pub fn default_op_policy(values: &[N], n: usize) -> Result<(CageOperator, T), Error> {
    match values.len() {
        0 => Err(Error::EmptyFills),
        1 => Ok((CageOperator::Given, T::from(values[0]))),
        2 => {
            let (hi, lo) = (values[0].max(values[1]), values[0].min(values[1]));
            if hi.is_multiple_of(lo) {
                Ok((CageOperator::Divide, T::from(hi / lo)))
            } else {
                Ok((CageOperator::Subtract, T::from(hi - lo)))
            }
        }
        _ => {
            let prod: T = values.iter().map(|&v| T::from(v)).product();
            let area = T::try_from(n * n).unwrap_or(T::MAX);
            if prod <= area {
                Ok((CageOperator::Multiply, prod))
            } else {
                Ok((CageOperator::Add, values.iter().map(|&v| T::from(v)).sum()))
            }
        }
    }
}

/// Generates a random `n×n` puzzle using the default operation policy and
/// a default Poisson size distribution.
///
/// # Errors
/// Returns `Error` if `n` is not in `1..=9`.
pub fn generate<R: Rng>(n: usize, rng: &mut R) -> Result<Puzzle, Error> {
    generate_with(n, rng, default_op_policy, SizeDistribution::default_for(n))
}

/// Generates a random `n×n` puzzle with caller-supplied op policy and
/// cage-size distribution.
///
/// # Errors
/// Returns `Error` if `n` is not in `1..=9`, or any error returned by `op`.
pub fn generate_with<R: Rng, F>(
    n: usize,
    rng: &mut R,
    op: F,
    sizes: SizeDistribution,
) -> Result<Puzzle, Error>
where
    F: Fn(&[N], usize) -> Result<(CageOperator, T), Error>,
{
    let mut puzzle = Puzzle::new(n)?;
    let latin_square = generate_latin_square(n, rng);
    let tiling = greedy(n, sizes, rng)?;

    for polyomino in tiling {
        // Cells are 1-based; latin_square is 0-indexed rows/cols.
        let values: Vec<N> = polyomino
            .iter()
            .map(|&Cell(r, c)| latin_square[r - 1][c - 1])
            .collect();
        let (operation, target) = op(&values, n)?;
        puzzle = puzzle
            .insert(&polyomino, operation, target)?
            .ok_or(Error::EmptyFills)?;
    }
    Ok(puzzle)
}

/// Builds a tiling that fully covers an `n×n` grid by greedy growth.
///
/// # Errors
/// Returns an error if a grown cell set fails polyomino validation.
pub fn greedy<R: Rng>(
    n: usize,
    dist: SizeDistribution,
    rng: &mut R,
) -> Result<Vec<Polyomino>, Error> {
    let mut tiling = Vec::new();
    let mut covered: HashSet<Cell> = HashSet::with_capacity(n * n);

    while covered.len() < n * n {
        let uncovered: Vec<Cell> = (1..=n)
            .flat_map(|r| (1..=n).map(move |c| Cell(r, c)))
            .filter(|c| !covered.contains(c))
            .collect();
        let seed = uncovered[rng.random_range(0..uncovered.len())];
        let target_size = dist.sample(n, rng);

        let mut cells: HashSet<Cell> = HashSet::new();
        let _ = cells.insert(seed);
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
        tiling.push(Polyomino::from(cells)?);
    }

    Ok(tiling)
}

/// In-bounds 4-neighbors of `cell` in an `n×n` grid (1-based).
fn grid_neighbors(cell: Cell, n: usize) -> impl Iterator<Item = Cell> {
    let Cell(r, c) = cell;
    [
        r.checked_sub(1).map(|r2| Cell(r2, c)),
        (r < n).then_some(Cell(r + 1, c)),
        c.checked_sub(1).map(|c2| Cell(r, c2)),
        (c < n).then_some(Cell(r, c + 1)),
    ]
    .into_iter()
    .flatten()
    // Filter out row/col 0 (below the 1-based minimum).
    .filter(|&Cell(r2, c2)| r2 >= 1 && c2 >= 1)
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    use super::*;
    use std::collections::HashSet;

    #[test]
    fn default_op_policy_one_cell_is_given() {
        assert!(matches!(
            default_op_policy(&[3], 4),
            Ok((CageOperator::Given, 3))
        ));
    }

    #[test]
    fn default_op_policy_two_cells_divisible_is_divide() {
        assert!(matches!(
            default_op_policy(&[2, 6], 6),
            Ok((CageOperator::Divide, 3))
        ));
    }

    #[test]
    fn default_op_policy_two_cells_not_divisible_is_subtract() {
        assert!(matches!(
            default_op_policy(&[2, 5], 6),
            Ok((CageOperator::Subtract, 3))
        ));
    }

    #[test]
    fn default_op_policy_three_cells_product_within_n_squared_is_multiply() {
        assert!(matches!(
            default_op_policy(&[1, 2, 3], 4),
            Ok((CageOperator::Multiply, 6))
        ));
    }

    #[test]
    fn default_op_policy_three_cells_product_above_n_squared_is_add() {
        assert!(matches!(
            default_op_policy(&[3, 4, 4], 4),
            Ok((CageOperator::Add, 11))
        ));
    }

    #[test]
    fn default_op_policy_overflowing_product_falls_back_to_add() {
        // 9^9 overflows; must fall back to Add. sum = 9*9 = 81.
        assert!(matches!(
            default_op_policy(&[9, 9, 9, 9, 9, 9, 9, 9, 9], 9),
            Ok((CageOperator::Add, 81))
        ));
    }

    #[test]
    fn default_op_policy_empty_returns_err() {
        assert!(matches!(default_op_policy(&[], 4), Err(Error::EmptyFills)));
    }

    #[test]
    fn greedy_covers_all_cells() {
        for seed in 0u64..200 {
            let mean = if seed % 2 == 0 { 1.0 } else { 2.5 };
            let dist = SizeDistribution { mean };
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let n = if seed % 3 == 0 { 4 } else { 3 };
            let tiling = greedy(n, dist, &mut rng).unwrap();
            let covered: HashSet<Cell> = tiling.iter().flat_map(|p| p.iter().copied()).collect();
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
