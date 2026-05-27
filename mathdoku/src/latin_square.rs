//! Uniformly random Latin square generation via the Jacobson–Matthews Markov chain.
//!
//! The entry point is [`generate_latin_square`].
//!
//! Reference: Mark T. Jacobson and Peter Matthews, "Generating uniformly distributed random Latin
//! squares", *Journal of Combinatorial Designs* 4(6), 1996, pp. 405–437.

#![allow(
    clippy::many_single_char_names,   // r/c/v/i/j/k are conventional for Latin-square indices
    clippy::cast_possible_truncation, // v+1 <= n <= 9 always fits in N (u8)
)]

use rand::{Rng, RngExt};

use crate::cell::N;

/// Returns a uniformly random index `x` in `0..n` such that `line(x) == 1`.
/// In a proper state each line has exactly one such entry; in an improper state
/// there are two, and we pick uniformly between them.
fn pick_one_from_line(rng: &mut impl Rng, n: usize, line: impl Fn(usize) -> i8) -> usize {
    let mut ones = [0usize; 2];
    let mut count = 0;
    for x in 0..n {
        if line(x) == 1 {
            ones[count] = x;
            count += 1;
        }
    }
    ones[rng.random_range(0..count)]
}

/// Generates a uniformly random n×n Latin square using the Jacobson-Matthews
/// Markov chain.
///
/// The state is an n×n×n incidence cube `m` where, in the *proper* regime,
/// `m[r][c][v] ∈ {0,1}` and `m[r][c][v] = 1` iff the underlying Latin square
/// has value `v+1` at cell `(r,c)`. Every line of `m` (fixing any two
/// coordinates) sums to 1. A move perturbs `m` by ±1 at the eight corners of a
/// 2×2×2 sub-cube, preserving every line sum. From a proper state the move
/// yields either another proper state or an *improper* state with a single −1
/// entry; from improper, the chain is biased to walk back to proper. Restricted
/// to proper states, the stationary distribution is uniform on n×n Latin
/// squares.
///
/// Burns in for `6*n³` steps (more than the original paper's heuristic of n³
/// to ensure thorough mixing), then continues until we land in a proper state.
///
/// Reference: Mark T. Jacobson and Peter Matthews, "Generating uniformly
/// distributed random Latin squares", *Journal of Combinatorial Designs* 4(6),
/// 1996, pp. 405–437.
pub fn generate_latin_square(n: usize, rng: &mut impl Rng) -> Vec<Vec<N>> {
    // Seed with the cyclic Latin square: L[r][c] = ((r + c) mod n) + 1.
    let mut m: Vec<Vec<Vec<i8>>> = vec![vec![vec![0i8; n]; n]; n];
    for r in 0..n {
        for c in 0..n {
            m[r][c][(r + c) % n] = 1;
        }
    }

    let mut improper: Option<(usize, usize, usize)> = None;
    let target_moves = 6 * n * n * n;
    let mut moves = 0usize;

    while moves < target_moves || improper.is_some() {
        let (i, j, k) = improper.unwrap_or_else(|| {
            loop {
                // Rejection-sample a 0-cell: (n³ − n²) of the n³ entries are
                // zero, so a draw is accepted with probability ≥ (n−1)/n.
                let r = rng.random_range(0..n);
                let c = rng.random_range(0..n);
                let v = rng.random_range(0..n);
                if m[r][c][v] == 0 {
                    break (r, c, v);
                }
            }
        });

        // Pick a 1-cell on each of the three lines through (i,j,k). In the
        // proper regime each line has a unique 1-cell; through an improper
        // entry there are exactly two and we pick uniformly.
        let ip = pick_one_from_line(rng, n, |x| m[x][j][k]);
        let jp = pick_one_from_line(rng, n, |x| m[i][x][k]);
        let kp = pick_one_from_line(rng, n, |x| m[i][j][x]);

        // ±1 perturbation around the (i,i')×(j,j')×(k,k') sub-cube:
        // +1 at corners with an even number of primed coordinates, −1 at odd.
        m[i][j][k] += 1;
        m[ip][j][k] -= 1;
        m[i][jp][k] -= 1;
        m[i][j][kp] -= 1;
        m[ip][jp][k] += 1;
        m[ip][j][kp] += 1;
        m[i][jp][kp] += 1;
        m[ip][jp][kp] -= 1;

        improper = (m[ip][jp][kp] == -1).then_some((ip, jp, kp));
        moves += 1;
    }

    (0..n)
        .map(|r| {
            (0..n)
                .map(|c| {
                    // The invariant guarantees exactly one 1 per line; this cannot be None.
                    let v = (0..n).position(|v| m[r][c][v] == 1).unwrap_or(0);
                    (v + 1) as N
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    use std::collections::{HashMap, HashSet};

    use super::*;

    /// Returns true if `ls` is a valid n×n Latin square: each row and each
    /// column contains each value in `1..=n` exactly once.
    fn validate_latin_square(ls: &[Vec<N>]) -> bool {
        let n = ls.len();
        let expected: HashSet<N> = (1..=(n as N)).collect();
        for row in ls {
            if row.iter().copied().collect::<HashSet<N>>() != expected {
                return false;
            }
        }
        for c in 0..n {
            let col: HashSet<N> = ls.iter().map(|r| r[c]).collect();
            if col != expected {
                return false;
            }
        }
        true
    }

    #[test]
    fn generate_4x4_returns_valid_square() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let ls = generate_latin_square(4, &mut rng);
        assert!(validate_latin_square(&ls));
    }

    #[test]
    fn validate_rejects_invalid() {
        // Row 0 has a duplicate value (two 1s), so this is not a valid Latin square.
        let ls = vec![vec![1u8, 1, 3], vec![2, 3, 1], vec![3, 2, 2]];
        assert!(!validate_latin_square(&ls));
    }

    #[test]
    fn generates_all_twelve_reduced_3x3_squares() {
        // There are exactly 12 distinct 3×3 Latin squares. With 1200 samples, every one
        // of them should appear at least once; tolerance is loose so the test
        // does not flake on rare seeds.
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let mut counts: HashMap<Vec<Vec<N>>, usize> = HashMap::new();
        for _ in 0..1200 {
            let ls = generate_latin_square(3, &mut rng);
            *counts.entry(ls).or_insert(0) += 1;
        }
        assert_eq!(counts.len(), 12);
        for (grid, &count) in &counts {
            assert!(count >= 10, "grid {grid:?} only appeared {count} times");
        }
    }

    #[test]
    fn validate_rejects_invalid_column() {
        // Every row is {1,2,3} (a valid permutation), but column 0 is {1,1,1}.
        let ls = vec![vec![1u8, 2, 3], vec![1, 3, 2], vec![1, 2, 3]];
        assert!(!validate_latin_square(&ls));
    }
}
