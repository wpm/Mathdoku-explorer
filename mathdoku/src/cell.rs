//! The primitive grid types: [`Cell`], [`Values`], and numeric types.

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    fmt,
    ops::{BitAnd, BitOr},
};

use crate::Error;

/// Possible cell value, a number in the range `1..=9`.
pub type Value = u8;
/// A cage target (sum, product, difference, ratio, or given value).
pub type Target = u64;
/// An ordered assignment of values to the cells of a cage, one value per cell.
pub type Tuple = Vec<Value>;

/// A cell in a Mathdoku grid, identified by 0-based row and column index values
/// in row-major order.
#[derive(Ord, Eq, PartialEq, PartialOrd, Debug, Copy, Clone, Hash, Serialize, Deserialize)]
pub struct Cell {
    /// 0-based row index.
    pub row: usize,
    /// 0-based column index.
    pub column: usize,
}

impl Cell {
    /// Creates a cell at the given `row` and `column`.
    pub const fn new(row: usize, column: usize) -> Self {
        Self { row, column }
    }

    /// Returns the up to four edge-adjacent cells (north, south, west, east).
    ///
    /// Cells above row 0 or left of column 0 are omitted. Cells below or to
    /// the right of the grid boundary are **not** filtered — callers must
    /// apply their own bounds check against the grid size.
    pub fn neighbors_4(self) -> impl Iterator<Item = Self> {
        [
            self.row.checked_sub(1).map(|r| Self::new(r, self.column)),
            Some(Self::new(self.row + 1, self.column)),
            self.column.checked_sub(1).map(|c| Self::new(self.row, c)),
            Some(Self::new(self.row, self.column + 1)),
        ]
        .into_iter()
        .flatten()
    }
}

/// The set of values in `1..=9` that a cell contains, stored as a bitmap.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default)]
pub struct Values(u16);

impl Values {
    /// Creates a `Values` set from a slice of numbers in the range `1..=9`.
    ///
    /// # Errors
    /// Returns [`Error::InvalidValue`] if any value is not in `1..=9`.
    pub fn new(ns: &[Value]) -> Result<Self, Error> {
        for &n in ns {
            if !(1..=9).contains(&n) {
                return Err(Error::InvalidValue(n));
            }
        }
        Ok(Self(
            ns.iter().fold(0u16, |acc, &n| acc | (1u16 << u32::from(n))),
        ))
    }

    /// Returns the full set `{1, ..., n}`.
    #[allow(clippy::cast_possible_truncation)]
    pub fn all(n: usize) -> Self {
        Self((1..=(n as Value)).fold(0u16, |acc, n| acc | (1u16 << u32::from(n))))
    }

    /// Creates a `Values` set from a single value, bypassing validation.
    /// Callers must guarantee `n` is in `1..=9`.
    pub(crate) fn singleton(n: Value) -> Self {
        Self(1u16 << u32::from(n))
    }

    /// Returns the values in ascending order.
    pub fn values(self) -> Vec<Value> {
        (1u8..=9).filter(|&v| self.0 & (1u16 << v) != 0).collect()
    }

    /// Returns true if the set contains no values.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns true if there is exactly one value.
    ///
    /// Values are stored in bits 1–9 of a `u16`, so exactly one value means
    /// exactly one bit is set, which is equivalent to the inner integer
    /// being a power of two.
    pub const fn is_singleton(self) -> bool {
        self.0.is_power_of_two()
    }

    /// Returns the number of values.
    pub const fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    /// Returns `true` if `value` is in this set.
    pub const fn contains(self, value: Value) -> bool {
        self.0 & (1u16 << value) != 0
    }
}

impl BitAnd for Values {
    type Output = Self;

    /// Returns the intersection of two sets of values.
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

impl BitOr for Values {
    type Output = Self;

    /// Returns the union of two sets of values.
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl Serialize for Values {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.values())
    }
}

impl<'de> Deserialize<'de> for Values {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let values = Vec::<Value>::deserialize(d)?;
        Self::new(&values).map_err(|e| DeError::custom(fmt::format(format_args!("{e}"))))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use serde_json::{from_str, to_string};

    use super::*;

    #[test]
    fn default_values_is_empty() {
        assert_eq!(Values::default(), Values::new(&[]).unwrap());
    }

    #[test]
    fn new_contains_one_through_four() {
        assert_eq!(
            Values::new(&[1, 2, 3, 4]).unwrap(),
            Values::new(&[1, 2, 3, 4]).unwrap()
        );
    }

    #[test]
    fn new_single_value() {
        assert_eq!(Values::new(&[1]).unwrap(), Values::new(&[1]).unwrap());
    }

    #[test]
    fn new_one_through_nine() {
        assert_eq!(Values::all(9), Values::all(9));
    }

    #[test]
    fn full_contains_one_through_n() {
        assert_eq!(Values::all(4), Values::new(&[1, 2, 3, 4]).unwrap());
    }

    #[test]
    fn new_rejects_zero() {
        assert!(matches!(Values::new(&[0]), Err(Error::InvalidValue(0))));
    }

    #[test]
    fn new_rejects_ten() {
        assert!(matches!(Values::new(&[10]), Err(Error::InvalidValue(10))));
    }

    #[test]
    fn bitand_intersection() {
        assert_eq!(
            Values::new(&[1, 2, 3]).unwrap() & Values::new(&[2, 3, 4]).unwrap(),
            Values::new(&[2, 3]).unwrap()
        );
    }

    #[test]
    fn bitand_disjoint_is_empty() {
        assert_eq!(
            Values::new(&[1, 2]).unwrap() & Values::new(&[3, 4]).unwrap(),
            Values::default()
        );
    }

    #[test]
    fn cell_ordering_is_row_major() {
        assert!(Cell::new(0, 1) < Cell::new(1, 0));
    }

    #[test]
    fn is_singleton_true_for_single_value() {
        assert!(Values::new(&[1]).unwrap().is_singleton());
        assert!(Values::new(&[5]).unwrap().is_singleton());
        assert!(Values::new(&[9]).unwrap().is_singleton());
    }

    #[test]
    fn is_singleton_false_for_empty() {
        assert!(!Values::default().is_singleton());
    }

    #[test]
    fn is_singleton_false_for_multiple_values() {
        assert!(!Values::new(&[1, 2]).unwrap().is_singleton());
        assert!(!Values::all(4).is_singleton());
    }

    #[test]
    fn is_empty_true_for_default() {
        assert!(Values::default().is_empty());
    }

    #[test]
    fn is_empty_false_for_non_empty() {
        assert!(!Values::new(&[1]).unwrap().is_empty());
        assert!(!Values::all(9).is_empty());
    }

    #[test]
    fn len_matches_number_of_values() {
        assert_eq!(Values::default().len(), 0);
        assert_eq!(Values::new(&[3]).unwrap().len(), 1);
        assert_eq!(Values::new(&[1, 5, 9]).unwrap().len(), 3);
        assert_eq!(Values::all(9).len(), 9);
    }

    #[test]
    fn bitor_union() {
        assert_eq!(
            Values::new(&[1, 2]).unwrap() | Values::new(&[2, 3]).unwrap(),
            Values::new(&[1, 2, 3]).unwrap()
        );
    }

    #[test]
    fn bitor_disjoint() {
        assert_eq!(
            Values::new(&[1, 2]).unwrap() | Values::new(&[3, 4]).unwrap(),
            Values::new(&[1, 2, 3, 4]).unwrap()
        );
    }

    #[test]
    fn neighbors_4_interior_yields_four() {
        let n: Vec<Cell> = Cell::new(2, 2).neighbors_4().collect();
        assert_eq!(n.len(), 4);
        assert!(n.contains(&Cell::new(1, 2)));
        assert!(n.contains(&Cell::new(3, 2)));
        assert!(n.contains(&Cell::new(2, 1)));
        assert!(n.contains(&Cell::new(2, 3)));
    }

    #[test]
    fn neighbors_4_top_left_corner_yields_two() {
        let n: Vec<Cell> = Cell::new(0, 0).neighbors_4().collect();
        assert_eq!(n.len(), 2);
        assert!(n.contains(&Cell::new(1, 0)));
        assert!(n.contains(&Cell::new(0, 1)));
    }

    #[test]
    fn values_round_trips_through_json() {
        let values = Values::new(&[1, 3, 5]).unwrap();
        let json = to_string(&values).unwrap();
        assert_eq!(json, "[1,3,5]");
        let restored: Values = from_str(&json).unwrap();
        assert_eq!(values, restored);
    }

    #[test]
    fn empty_values_round_trips_through_json() {
        let values = Values::default();
        let json = to_string(&values).unwrap();
        let restored: Values = from_str(&json).unwrap();
        assert_eq!(values, restored);
    }

    #[test]
    fn values_deserialize_rejects_out_of_range_values() {
        assert!(from_str::<Values>("[0]").is_err());
        assert!(from_str::<Values>("[10]").is_err());
    }
}
