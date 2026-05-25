use crate::Error;
use crate::cell::{M, N};

/// An ordered assignment of values to the cells of a cage, one value per cell.
pub type Tuple = Vec<N>;

/// Returns an iterator over all non-decreasing `k`-tuples with values in `1..=n` that sum to `s`.
pub fn addition_multisets(n: N, k: usize, s: N) -> impl Iterator<Item = Tuple> {
    simplex_multisets(n, k, |acc, i| acc + u64::from(i), u64::from(s))
}

/// Returns an iterator over all 2-element tuples `[i, j]` with `j - i == d` and `1 <= i < j <= max`.
pub fn subtraction_multisets(max: N, d: N) -> impl Iterator<Item = Tuple> {
    (1..=max.saturating_sub(d)).map(move |i| vec![i, i + d])
}

/// Returns an iterator over all non-decreasing `k`-tuples with values in `1..=n` whose product is `s`.
pub fn multiplication_multisets(n: N, k: usize, s: M) -> impl Iterator<Item = Tuple> {
    simplex_multisets(n, k, |acc, i| acc * u64::from(i), u64::from(s))
}

/// Returns an iterator over all 2-element tuples `[i, j]` with `j / i == q` and `1 <= i < j <= max`.
pub fn division_multisets(max: N, q: N) -> impl Iterator<Item = Tuple> {
    (1..=max / q).map(move |i| vec![i, i * q])
}

/// Returns an iterator over all non-decreasing `tuple_size`-tuples with values
/// in `1..=n` where applying `f` across the tuple (left fold) equals `s`.
///
/// `f` folds a `u64` accumulator over each `N` element. This matters for
/// multiplication: the worst case is a 9-cell cage on a 9×9 grid whose product
/// could reach 9⁹ ≈ 3.9×10⁸, which overflows `M` (`u16`) but fits in `u64`.
/// Because the target `s` is also widened to `u64`, any partial product that
/// exceeds `M::MAX` simply fails the `v <= s` pruning check — no special-casing
/// needed. Complete tuples are yielded only when their fold value equals `s`.
fn simplex_multisets(
    n: N,
    tuple_size: usize,
    f: impl Fn(u64, N) -> u64 + Copy + 'static,
    s: u64,
) -> Box<dyn Iterator<Item = Tuple>> {
    simplex_multisets_inner(n, tuple_size, tuple_size, f, s)
}

/// Recursive worker for [`simplex_multisets`].
///
/// Builds tuples of length `total_size` one element at a time. `remaining`
/// counts how many positions are still to be filled. Each recursive call
/// extends prefixes of length `total_size - remaining` by one element, pruning
/// branches whose accumulated value already exceeds `s`.
#[allow(clippy::many_single_char_names)]
fn simplex_multisets_inner(
    n: N,
    total_size: usize,
    remaining: usize,
    f: impl Fn(u64, N) -> u64 + Copy + 'static,
    s: u64,
) -> Box<dyn Iterator<Item = Tuple>> {
    if remaining == 0 {
        return Box::new(std::iter::once(vec![]));
    }
    Box::new(
        simplex_multisets_inner(n, total_size, remaining - 1, f, s).flat_map(move |t| {
            let last = t.last().copied().unwrap_or(1);
            (last..=n).filter_map(move |i| {
                let mut t = t.clone();
                t.push(i);
                sequence_operation(f, &t).ok().and_then(|v| {
                    if t.len() == total_size {
                        (v == s).then_some(t)
                    } else {
                        (v <= s).then_some(t)
                    }
                })
            })
        }),
    )
}

/// Reduces `t` by left-folding `f` over its elements, starting from the first
/// element widened to `u64`. Returns [`Error::EmptyTuple`] if `t` is empty.
fn sequence_operation(f: impl Fn(u64, N) -> u64, t: &[N]) -> Result<u64, Error> {
    let Some((&t_0, rest)) = t.split_first() else {
        return Err(Error::EmptyTuple);
    };
    Ok(rest.iter().fold(u64::from(t_0), |acc, &i| f(acc, i)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn subtraction_multisets_target_1_min_1() {
        itertools::assert_equal(
            subtraction_multisets(4, 1),
            [vec![1, 2], vec![2, 3], vec![3, 4]],
        );
    }

    #[test]
    fn subtraction_multisets_target_2_min_1() {
        itertools::assert_equal(subtraction_multisets(4, 2), [vec![1, 3], vec![2, 4]]);
    }

    #[test]
    fn subtraction_multisets_no_results() {
        itertools::assert_equal(subtraction_multisets(3, 5), Vec::<Tuple>::new());
    }

    #[test]
    fn division_multisets_target_2_min_1() {
        itertools::assert_equal(division_multisets(4, 2), [vec![1, 2], vec![2, 4]]);
    }

    #[test]
    fn division_multisets_target_3_min_1() {
        itertools::assert_equal(division_multisets(6, 3), [vec![1, 3], vec![2, 6]]);
    }

    #[test]
    fn division_multisets_no_results() {
        itertools::assert_equal(division_multisets(5, 7), Vec::<Tuple>::new());
    }

    #[test]
    fn simplex_multisets_filters_by_sum() {
        // k=2, n=4, sum=5: pairs are [1,4], [2,3]
        itertools::assert_equal(addition_multisets(4, 2, 5), [vec![1, 4], vec![2, 3]]);
    }

    #[test]
    fn simplex_multisets_filters_by_product() {
        // k=2, n=6, product=6: pairs are [1,6], [2,3]
        itertools::assert_equal(multiplication_multisets(6, 2, 6), [vec![1, 6], vec![2, 3]]);
    }

    #[test]
    fn addition_multisets_k3() {
        // n=4, k=3, s=6: [1,1,4], [1,2,3], [2,2,2]
        itertools::assert_equal(
            addition_multisets(4, 3, 6),
            [vec![1, 1, 4], vec![1, 2, 3], vec![2, 2, 2]],
        );
    }

    #[test]
    fn addition_multisets_no_results() {
        // s=1 with k=2 is impossible since min sum is 1+1=2
        itertools::assert_equal(addition_multisets(4, 2, 1), Vec::<Tuple>::new());
    }

    #[test]
    fn multiplication_multisets_k2() {
        // n=6, k=2, s=6: [1,6], [2,3]
        itertools::assert_equal(multiplication_multisets(6, 2, 6), [vec![1, 6], vec![2, 3]]);
    }

    #[test]
    fn multiplication_multisets_k3() {
        // n=4, k=3, s=8: [1,2,4], [2,2,2]
        itertools::assert_equal(
            multiplication_multisets(4, 3, 8),
            [vec![1, 2, 4], vec![2, 2, 2]],
        );
    }

    #[test]
    fn multiplication_multisets_no_results() {
        // s=7 is prime, so no factorization into 3 values in 1..=4
        itertools::assert_equal(multiplication_multisets(4, 3, 7), Vec::<Tuple>::new());
    }

    #[test]
    fn multiplication_multisets_large_product() {
        // 9^2 = 81 fits in N, but 9^3 = 729 does not — requires M
        itertools::assert_equal(multiplication_multisets(9, 3, 729), [vec![9, 9, 9]]);
    }

    #[test]
    fn multiplication_multisets_overflow_product_returns_empty() {
        // 9^9 = 387_420_489 overflows M (u16); no tuple should match.
        assert!(multiplication_multisets(9, 9, u16::MAX).next().is_none());
    }

    fn add_acc(a: u64, b: N) -> u64 {
        a + u64::from(b)
    }

    #[test]
    fn sequence_operation_with_named_function_folds_correctly() {
        assert_eq!(sequence_operation(add_acc, &[3, 4, 5]).unwrap(), 12);
    }

    #[test]
    fn sequence_operation_errors_on_empty_tuple() {
        assert!(matches!(
            sequence_operation(add_acc, &Tuple::new()),
            Err(Error::EmptyTuple)
        ));
    }
}
