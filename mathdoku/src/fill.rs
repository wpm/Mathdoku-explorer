//! Candidate value sets.
use crate::Error;
use crate::N;
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};
use std::ops::{BitAnd, BitOr};

/// The set of candidate values for a cell, stored as a u16 bitmap.
///
/// Bit `v` (1 ≤ v ≤ 9) is set iff value `v` is a candidate. Bit 0 is unused.
#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone, Debug, Default, Hash)]
pub struct Fill(u16);

impl Fill {
    /// Creates a full candidate set `{1..=n}`.
    #[must_use]
    pub const fn all(n: usize) -> Self {
        // Set bits 1..=n: ((1 << (n+1)) - 1) & !1
        Self(((1u16 << (n + 1)).wrapping_sub(1)) & !1)
    }

    /// Creates a candidate set from a slice of values in `1..=9`.
    ///
    /// # Errors
    /// Returns [`Error::InvalidValue`] if any value is outside `1..=9`.
    pub fn new(ns: &[N]) -> Result<Self, Error> {
        for &n in ns {
            if !(1..=9).contains(&n) {
                return Err(Error::InvalidValue(n));
            }
        }
        Ok(Self(ns.iter().fold(0u16, |acc, &v| acc | (1u16 << v))))
    }

    /// Creates a candidate set from an explicit slice of values without validation.
    pub(crate) fn from(ns: &[N]) -> Self {
        Self(ns.iter().fold(0u16, |acc, &v| acc | (1u16 << v)))
    }

    /// Creates a singleton set `{n}` without validation. Callers must ensure `n` is in `1..=9`.
    #[must_use]
    pub const fn singleton(n: N) -> Self {
        Self(1u16 << n)
    }

    /// Returns `true` if `value` is in this candidate set.
    #[must_use]
    pub const fn contains(self, value: N) -> bool {
        self.0 & (1u16 << value) != 0
    }

    /// Returns `true` if the set is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns `true` if the set contains exactly one value.
    #[must_use]
    pub const fn is_singleton(self) -> bool {
        self.0.is_power_of_two()
    }

    /// Returns the number of values in the set.
    #[must_use]
    pub const fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    /// Returns the values in ascending order.
    #[must_use]
    pub fn values(self) -> Vec<N> {
        (1u8..=9).filter(|&v| self.0 & (1u16 << v) != 0).collect()
    }

    /// Returns the smallest value in the set, or `None` if empty.
    #[must_use]
    pub fn min_value(self) -> Option<N> {
        (1u8..=9).find(|&v| self.0 & (1u16 << v) != 0)
    }

    /// Returns the largest value in the set, or `None` if empty.
    #[must_use]
    pub fn max_value(self) -> Option<N> {
        (1u8..=9).rev().find(|&v| self.0 & (1u16 << v) != 0)
    }
}

impl BitOr for Fill {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitAnd for Fill {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

impl crate::csp::Domain for Fill {
    fn is_empty(&self) -> bool {
        Self::is_empty(*self)
    }
}

impl Display for Fill {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{")?;
        let mut first = true;
        for v in self.values() {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "{v}")?;
            first = false;
        }
        write!(f, "}}")
    }
}

impl Serialize for Fill {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.values())
    }
}

impl<'de> Deserialize<'de> for Fill {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let ns = Vec::<N>::deserialize(d)?;
        Self::new(&ns).map_err(|e| DeError::custom(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{from_str, to_string};

    #[test]
    fn all_contains_one_through_n() {
        let f = Fill::all(4);
        assert!(f.contains(1));
        assert!(f.contains(4));
        assert!(!f.contains(0));
        assert!(!f.contains(5));
    }

    #[test]
    fn new_empty_slice_is_empty() {
        assert!(Fill::new(&[]).unwrap().is_empty());
    }

    #[test]
    fn new_deduplicates_values() {
        assert_eq!(Fill::new(&[2, 2, 3]).unwrap(), Fill::from(&[2, 3]));
    }

    #[test]
    fn new_rejects_zero() {
        assert!(matches!(Fill::new(&[0]), Err(Error::InvalidValue(0))));
    }

    #[test]
    fn new_rejects_ten() {
        assert!(matches!(Fill::new(&[10]), Err(Error::InvalidValue(10))));
    }

    #[test]
    fn singleton_contains_only_that_value() {
        let f = Fill::singleton(3);
        assert!(f.contains(3));
        assert!(!f.contains(2));
        assert!(f.is_singleton());
    }

    #[test]
    fn from_empty_slice_is_empty() {
        assert!(Fill::from(&[]).is_empty());
    }

    #[test]
    fn contains_absent_value_is_false() {
        assert!(!Fill::from(&[1, 3]).contains(2));
    }

    #[test]
    fn is_empty_false_for_non_empty() {
        assert!(!Fill::all(3).is_empty());
    }

    #[test]
    fn is_singleton_true_for_single_value() {
        assert!(Fill::new(&[5]).unwrap().is_singleton());
    }

    #[test]
    fn is_singleton_false_for_multiple() {
        assert!(!Fill::new(&[1, 2]).unwrap().is_singleton());
    }

    #[test]
    fn is_singleton_false_for_empty() {
        assert!(!Fill::default().is_singleton());
    }

    #[test]
    fn len_counts_values() {
        assert_eq!(Fill::default().len(), 0);
        assert_eq!(Fill::singleton(3).len(), 1);
        assert_eq!(Fill::new(&[1, 5, 9]).unwrap().len(), 3);
        assert_eq!(Fill::all(9).len(), 9);
    }

    #[test]
    fn default_is_empty() {
        assert!(Fill::default().is_empty());
    }

    #[test]
    fn display_empty() {
        assert_eq!(Fill::from(&[]).to_string(), "{}");
    }

    #[test]
    fn display_singleton() {
        assert_eq!(Fill::from(&[3]).to_string(), "{3}");
    }

    #[test]
    fn display_sorted() {
        assert_eq!(Fill::from(&[3, 1, 2]).to_string(), "{1, 2, 3}");
    }

    #[test]
    fn round_trips_through_json() {
        let f = Fill::from(&[1, 3]);
        assert_eq!(from_str::<Fill>(&to_string(&f).unwrap()).unwrap(), f);
    }

    #[test]
    fn serialize_is_sorted_array() {
        assert_eq!(to_string(&Fill::from(&[3, 1])).unwrap(), r"[1,3]");
    }

    #[test]
    fn deserialize_rejects_out_of_range() {
        assert!(from_str::<Fill>("[0]").is_err());
        assert!(from_str::<Fill>("[10]").is_err());
    }

    #[test]
    fn bitor_union() {
        assert_eq!(
            Fill::new(&[1, 2]).unwrap() | Fill::new(&[2, 3]).unwrap(),
            Fill::new(&[1, 2, 3]).unwrap()
        );
    }

    #[test]
    fn bitand_intersection() {
        assert_eq!(
            Fill::new(&[1, 2, 3]).unwrap() & Fill::new(&[2, 3, 4]).unwrap(),
            Fill::new(&[2, 3]).unwrap()
        );
    }

    #[test]
    fn values_in_order() {
        assert_eq!(Fill::from(&[3, 1, 2]).values(), vec![1, 2, 3]);
    }
}
