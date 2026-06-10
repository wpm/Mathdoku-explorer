//! Explicit-tuple implementation of [`Memo`].
use crate::Error::{EmptyFills, InvalidCellCageIndex};
use crate::fill::Fill;
use crate::memo::Memo;
use crate::operator::{ArithmeticConstraint, CommutativeOperator, NonCommutativeOperator};
use crate::tuples::{Tuple, Tuples};
use crate::{Error, N, T};

/// A cage constraint stored as an explicit list of valid value tuples.
///
/// Each tuple is a `k`-vector of values in `1..=n` satisfying the cage's
/// arithmetic constraint. Per-position candidate sets ([`Fill`]s) are derived
/// as the union of values appearing at each position across all tuples, and
/// are guaranteed non-empty — construction fails with [`EmptyFills`]
/// if no valid tuples exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    n: N,
    constraint: ArithmeticConstraint,
    tuples: Vec<Vec<N>>,
    fills: Vec<Fill>,
}

impl Table {
    /// Constructs a representation of all `k`-tuples of values in `1..=n`
    /// satisfying a commutative (add or multiply) constraint.
    ///
    /// Commutative cages use [`Mdd`] in production; this constructor exists as a
    /// test utility to verify `Table` behaviour independently of operator class.
    ///
    /// # Errors
    /// Returns [`EmptyFills`] if no tuples satisfy the constraint.
    #[allow(dead_code)]
    pub fn commutative(
        n: N,
        k: N,
        operator: CommutativeOperator,
        target: T,
    ) -> Result<Self, Error> {
        let constraint = ArithmeticConstraint::CommutativeConstraint(operator, target);
        Self::build(
            n,
            constraint,
            Tuples::commutative(n, k, operator, target).collect(),
        )
    }

    /// Constructs a representation of all pairs of values in `1..=n`
    /// satisfying a non-commutative (subtract or divide) constraint.
    ///
    /// # Errors
    /// Returns [`EmptyFills`] if no tuples satisfy the constraint.
    pub fn non_commutative(
        n: N,
        operator: NonCommutativeOperator,
        target: T,
    ) -> Result<Self, Error> {
        let constraint = ArithmeticConstraint::NonCommutativeConstraint(operator, target);
        Self::build(
            n,
            constraint,
            Tuples::non_commutative(n, operator, target).collect(),
        )
    }

    pub(crate) fn tuples(&self) -> &[Vec<N>] {
        &self.tuples
    }

    /// Constructs a `Table` from a pre-computed list of tuples, deriving fills.
    ///
    /// # Errors
    /// Returns [`EmptyFills`] if `tuples` is empty or any position's
    /// fill would be empty.
    fn build(n: N, constraint: ArithmeticConstraint, tuples: Vec<Vec<N>>) -> Result<Self, Error> {
        let fills = fills_from_tuples(&tuples)?;
        Ok(Self {
            n,
            constraint,
            tuples,
            fills,
        })
    }
}

impl Memo for Table {
    fn get(&self, index: usize) -> Result<Fill, Error> {
        self.fills
            .get(index)
            .copied()
            .ok_or(InvalidCellCageIndex(index))
    }

    fn narrow(&self, support: &[Fill]) -> Result<Self, Error> {
        let tuples = self
            .tuples
            .iter()
            .filter(|tuple| {
                tuple
                    .iter()
                    .enumerate()
                    .all(|(i, &v)| support[i].contains(v))
            })
            .cloned()
            .collect::<Vec<_>>();
        Self::build(self.n, self.constraint, tuples)
    }
}

/// Derives per-position fills from a non-empty tuple list.
///
/// Returns `Err(EmptyFills)` if `tuples` is empty or any column's fill is empty.
pub fn fills_from_tuples(tuples: &[Tuple]) -> Result<Vec<Fill>, Error> {
    if tuples.is_empty() {
        return Err(EmptyFills);
    }
    let k = tuples[0].len();
    let fills: Vec<Fill> = (0..k)
        .map(|i| Fill::from(&tuples.iter().map(|t| t[i]).collect::<Tuple>()))
        .collect();
    if fills.iter().any(|f| f.is_empty()) {
        return Err(EmptyFills);
    }
    Ok(fills)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error::EmptyFills;
    use crate::operator::CommutativeOperator::{Add, Multiply};
    use crate::operator::NonCommutativeOperator::{Divide, Subtract};

    // ---- get ----

    #[test]
    fn add_fills_are_union_of_column_values() {
        // 3+3=6, 2+4=6, 4+2=6 — position 0 is {2,3,4}, position 1 is {2,3,4}
        let t = Table::commutative(4, 2, Add, 6).unwrap();
        assert_eq!(t.get(0).unwrap(), Fill::from(&[2, 3, 4]));
        assert_eq!(t.get(1).unwrap(), Fill::from(&[2, 3, 4]));
    }

    #[test]
    fn multiply_fills_contain_expected_values() {
        // 2*3=6, 3*2=6, 1*6=6, 6*1=6 within n=6
        let t = Table::commutative(6, 2, Multiply, 6).unwrap();
        assert_eq!(t.get(0).unwrap(), Fill::from(&[1, 2, 3, 6]));
        assert_eq!(t.get(1).unwrap(), Fill::from(&[1, 2, 3, 6]));
    }

    #[test]
    fn subtract_fills_contain_expected_values() {
        // pairs with |a-b|=1 in n=4: (1,2),(2,1),(2,3),(3,2),(3,4),(4,3)
        let t = Table::non_commutative(4, Subtract, 1).unwrap();
        assert_eq!(t.get(0).unwrap(), Fill::from(&[1, 2, 3, 4]));
        assert_eq!(t.get(1).unwrap(), Fill::from(&[1, 2, 3, 4]));
    }

    #[test]
    fn divide_fills_contain_expected_values() {
        // pairs with max/min=2 in n=4: (1,2),(2,1),(2,4),(4,2)
        let t = Table::non_commutative(4, Divide, 2).unwrap();
        assert_eq!(t.get(0).unwrap(), Fill::from(&[1, 2, 4]));
        assert_eq!(t.get(1).unwrap(), Fill::from(&[1, 2, 4]));
    }

    #[test]
    fn commutative_no_solutions_returns_empty_fills_error() {
        // no 2-tuple in 1..=4 sums to 9
        assert!(matches!(Table::commutative(4, 2, Add, 9), Err(EmptyFills)));
    }

    #[test]
    fn fill_out_of_bounds_returns_index_error() {
        let t = Table::commutative(4, 2, Add, 5).unwrap();
        assert!(matches!(t.get(2), Err(InvalidCellCageIndex(2))));
    }

    // ---- narrow ----

    #[test]
    fn narrow_with_full_support_is_identity() {
        // support that includes every value leaves all tuples intact
        let t = Table::commutative(4, 2, Add, 5).unwrap();
        let full = vec![Fill::all(4), Fill::all(4)];
        assert_eq!(t.narrow(&full).unwrap(), t);
    }

    #[test]
    fn narrow_filters_tuples_and_updates_fills() {
        // add to 5 in n=4: (1,4),(2,3),(3,2),(4,1)
        let t = Table::commutative(4, 2, Add, 5).unwrap();
        // restrict position 0 to {1,2}, position 1 to {1,2,3,4}
        let narrowed = t
            .narrow(&[Fill::from(&[1, 2]), Fill::from(&[1, 2, 3, 4])])
            .unwrap();
        assert_eq!(narrowed.get(0).unwrap(), Fill::from(&[1, 2]));
        assert_eq!(narrowed.get(1).unwrap(), Fill::from(&[3, 4]));
    }

    #[test]
    fn narrow_eliminating_all_tuples_returns_empty_fills_error() {
        let t = Table::commutative(4, 2, Add, 5).unwrap();
        // restrict both positions to {1} — no tuple (1,1) sums to 5
        assert!(matches!(
            t.narrow(&[Fill::from(&[1]), Fill::from(&[1])]),
            Err(EmptyFills)
        ));
    }
}
