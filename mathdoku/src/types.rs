use std::{
    fmt,
    ops::{BitAnd, BitOr},
};

use crate::{cage::Cage, operation::Operation, polyomino::Polyomino, slot::Slot};

/// Possible cell value: a number in the range `1..=9`.
pub type N = u8;
/// A cage target (sum, product, difference, ratio, or given value). Wide enough
/// to hold the largest possible product for a single cage.
pub type M = u16;

/// A cell in a Mathdoku grid, identified by 0-based row and column `Index` values
/// in row-major order.
#[derive(
    Ord, Eq, PartialEq, PartialOrd, Debug, Copy, Clone, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Cell {
    /// 0-based row index.
    pub row: Index,
    /// 0-based column index.
    pub column: Index,
}

impl Cell {
    /// Creates a cell at the given `row` and `column`.
    pub const fn new(row: Index, column: Index) -> Self {
        Self { row, column }
    }

    /// The (up to four) edge-connected cells, with no upper bound check.
    /// Cells off the top or left edge are filtered; cells off the bottom or
    /// right are not.
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
/// A 0-based row or column index.
pub type Index = usize;

/// A cell's domain: a set of values in `1..=9` stored as a bitmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Domain(u16);

impl Domain {
    /// Creates `Domain` from any iterable of numbers in the range `1..=9`.
    pub fn new(ns: impl IntoIterator<Item = N>) -> Self {
        Self(
            ns.into_iter()
                .fold(0u16, |acc, n| acc | (1u16 << u32::from(n))),
        )
    }

    /// Returns the full set `{1, ..., n}`.
    #[allow(clippy::cast_possible_truncation)]
    pub fn full(n: Index) -> Self {
        Self::new(1..=(n as N))
    }

    /// Returns an iterator over the values in ascending order.
    pub fn iter(self) -> impl Iterator<Item = N> {
        (1u8..=9).filter(move |&v| self.0 & (1u16 << v) != 0)
    }

    /// Returns true if the set contains no values.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns true if exactly one value is set.
    ///
    /// Values are stored in bits 1–9 of a `u16`, so exactly one value set means
    /// exactly one bit is set, which is equivalent to the inner integer
    /// being a power of two.
    pub const fn is_singleton(self) -> bool {
        self.0.is_power_of_two()
    }

    /// Returns the number of values in the domain.
    pub const fn len(self) -> usize {
        self.0.count_ones() as usize
    }
}

impl BitAnd for Domain {
    type Output = Self;

    /// Returns the intersection of two sets of values.
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

impl FromIterator<N> for Domain {
    fn from_iter<I: IntoIterator<Item = N>>(iter: I) -> Self {
        Self::new(iter)
    }
}

impl BitOr for Domain {
    type Output = Self;

    /// Returns the union of two sets of values.
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl serde::Serialize for Domain {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.iter())
    }
}

impl<'de> serde::Deserialize<'de> for Domain {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let values = Vec::<N>::deserialize(d)?;
        for &v in &values {
            if !(1..=9).contains(&v) {
                return Err(serde::de::Error::custom(format!(
                    "Domain value {v} is out of range 1..=9"
                )));
            }
        }
        Ok(Self::new(values))
    }
}

/// Errors that can occur during puzzle construction or solving.
#[derive(Debug)]
pub enum Error {
    /// A [`Puzzle`](crate::Puzzle) was constructed with a size less than 1 or greater than 9.
    InvalidGridSize(Index),
    /// A referenced [`Cell`] is not present in the grid.
    InvalidCell(Cell),
    /// A new [`Cage`] conflicts with an existing cage.
    CageConflict(Cage),
    /// A [`Slot`] passed to a `Puzzle` constructor covers cells outside the
    /// grid, so it cannot belong to that puzzle.
    SlotNotInPuzzle(Slot),
    /// Two slots passed to a `Puzzle` constructor share the same
    /// [`Polyomino`]. Allowed shapes must be distinct: storing both would
    /// silently collide in the puzzle's slot set ([`Slot::cmp`] keys on the
    /// polyomino).
    DuplicateSlotPolyomino(Polyomino),
    /// A new region [`Polyomino`] conflicts with an existing slot in the puzzle.
    RegionConflict(Polyomino),
    /// A [`Polyomino`] cannot support the requested [`Operation`]: either the
    /// operator is invalid for the cell count, or the target is unreachable.
    InfeasibleOperation(Polyomino, Operation),
    /// A tiling operation referenced a [`Cell`] that no polyomino covers.
    CellNotCovered(Cell),
    /// Removing a [`Cell`] from a [`crate::Polyomino`] would leave the remaining cells
    /// disconnected.
    WouldDisconnect(Cell),
    /// A target [`Cell`] is not edge-connected to any cell of the
    /// [`crate::Polyomino`] it was applied to.
    TargetNotAdjacent,
    /// A [`Cell`] passed to <code>[crate::Polyomino]::insert</code> is already in the polyomino.
    CellAlreadyInPolyomino(Cell),
    /// A <code>[crate::Polyomino]::remove</code> call would remove
    /// the polyomino's only remaining cell.
    RemovalWouldEmptyPolyomino(Cell),
    /// A [`crate::Polyomino`] was constructed from an empty cell slice.
    EmptyPolyomino,
    /// A [`crate::Polyomino`] was constructed from
    /// cells that are not edge-connected (e.g. a diagonal-only pair).
    DisconnectedPolyomino,
    /// A row/column index used to build an internal all-different constraint
    /// is not less than the grid size `n`. Carries `(index, n)`.
    IndexOutOfRange(Index, Index),
    /// An operation policy received an empty value slice.
    EmptyOpPolicyValues,
    /// A tuple operation was applied to an empty slice.
    EmptyTuple,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidGridSize(n) => write!(f, "invalid grid size {n}"),
            Self::InvalidCell(c) => {
                write!(f, "cell ({}, {}) is outside the grid", c.row, c.column)
            }
            Self::CageConflict(new) => {
                write!(
                    f,
                    "cage {new:?} conflicts with an existing cage in the puzzle"
                )
            }
            Self::SlotNotInPuzzle(slot) => match slot {
                Slot::Cage(c) => write!(f, "cage {c:?} is not in this puzzle"),
                Slot::Region(p) => write!(f, "region {p:?} is not in this puzzle"),
            },
            Self::DuplicateSlotPolyomino(p) => {
                write!(f, "duplicate polyomino {p:?} across puzzle slots")
            }
            Self::RegionConflict(p) => write!(
                f,
                "region {p:?} conflicts with an existing slot in the puzzle"
            ),
            Self::InfeasibleOperation(p, op) => {
                write!(f, "operation {op:?} is infeasible for polyomino {p:?}")
            }
            Self::CellNotCovered(c) => write!(
                f,
                "cell ({}, {}) is not covered by any polyomino",
                c.row, c.column
            ),
            Self::WouldDisconnect(c) => write!(
                f,
                "removing cell ({}, {}) would disconnect the polyomino",
                c.row, c.column
            ),
            Self::TargetNotAdjacent => {
                write!(f, "target cell is not edge-adjacent to the polyomino")
            }
            Self::CellAlreadyInPolyomino(c) => write!(
                f,
                "cell ({}, {}) is already in the polyomino",
                c.row, c.column
            ),
            Self::RemovalWouldEmptyPolyomino(c) => write!(
                f,
                "removing cell ({}, {}) would leave an empty polyomino",
                c.row, c.column
            ),
            Self::EmptyPolyomino => write!(
                f,
                "polyomino cannot be constructed from an empty cell slice"
            ),
            Self::DisconnectedPolyomino => write!(f, "polyomino cells are not edge-connected"),
            Self::IndexOutOfRange(index, n) => {
                write!(f, "index {index} is out of range for grid of size {n}")
            }
            Self::EmptyOpPolicyValues => {
                write!(f, "operation policy received an empty value slice")
            }
            Self::EmptyTuple => {
                write!(f, "tuple operation cannot be applied to an empty tuple")
            }
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn default_values_is_empty() {
        assert_eq!(Domain::default(), Domain::new([]));
    }

    #[test]
    fn new_contains_one_through_four() {
        assert_eq!(Domain::new(1..=4), Domain::new([1, 2, 3, 4]));
    }

    #[test]
    fn new_single_value() {
        assert_eq!(Domain::new([1]), Domain::new([1]));
    }

    #[test]
    fn new_one_through_nine() {
        assert_eq!(Domain::new(1..=9), Domain::full(9));
    }

    #[test]
    fn full_contains_one_through_n() {
        assert_eq!(Domain::full(4), Domain::new([1, 2, 3, 4]));
    }

    #[test]
    fn bitand_intersection() {
        assert_eq!(
            Domain::new([1, 2, 3]) & Domain::new([2, 3, 4]),
            Domain::new([2, 3])
        );
    }

    #[test]
    fn bitand_disjoint_is_empty() {
        assert_eq!(Domain::new([1, 2]) & Domain::new([3, 4]), Domain::default());
    }

    #[test]
    fn cell_ordering_is_row_major() {
        assert!(Cell::new(0, 1) < Cell::new(1, 0));
    }

    #[test]
    fn is_singleton_true_for_single_value() {
        assert!(Domain::new([1]).is_singleton());
        assert!(Domain::new([5]).is_singleton());
        assert!(Domain::new([9]).is_singleton());
    }

    #[test]
    fn is_singleton_false_for_empty() {
        assert!(!Domain::default().is_singleton());
    }

    #[test]
    fn is_singleton_false_for_multiple_values() {
        assert!(!Domain::new([1, 2]).is_singleton());
        assert!(!Domain::full(4).is_singleton());
    }

    #[test]
    fn is_empty_true_for_default() {
        assert!(Domain::default().is_empty());
    }

    #[test]
    fn is_empty_false_for_non_empty() {
        assert!(!Domain::new([1]).is_empty());
        assert!(!Domain::full(9).is_empty());
    }

    #[test]
    fn len_matches_number_of_values() {
        assert_eq!(Domain::default().len(), 0);
        assert_eq!(Domain::new([3]).len(), 1);
        assert_eq!(Domain::new([1, 5, 9]).len(), 3);
        assert_eq!(Domain::full(9).len(), 9);
    }

    #[test]
    fn bitor_union() {
        assert_eq!(
            Domain::new([1, 2]) | Domain::new([2, 3]),
            Domain::new([1, 2, 3])
        );
    }

    #[test]
    fn bitor_disjoint() {
        assert_eq!(
            Domain::new([1, 2]) | Domain::new([3, 4]),
            Domain::new([1, 2, 3, 4])
        );
    }

    #[test]
    fn from_iterator_collects_values() {
        let v: Domain = [1u8, 2, 3].into_iter().collect();
        assert_eq!(v, Domain::new([1, 2, 3]));
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
    fn error_display_covers_all_variants() {
        use crate::Error;
        let c = Cell::new(1, 2);
        assert_eq!(Error::InvalidGridSize(0).to_string(), "invalid grid size 0");
        assert_eq!(
            Error::InvalidCell(c).to_string(),
            "cell (1, 2) is outside the grid"
        );
        assert_eq!(
            Error::CellNotCovered(c).to_string(),
            "cell (1, 2) is not covered by any polyomino"
        );
        assert_eq!(
            Error::WouldDisconnect(c).to_string(),
            "removing cell (1, 2) would disconnect the polyomino"
        );
        assert_eq!(
            Error::TargetNotAdjacent.to_string(),
            "target cell is not edge-adjacent to the polyomino"
        );
        assert_eq!(
            Error::CellAlreadyInPolyomino(c).to_string(),
            "cell (1, 2) is already in the polyomino"
        );
        assert_eq!(
            Error::RemovalWouldEmptyPolyomino(c).to_string(),
            "removing cell (1, 2) would leave an empty polyomino"
        );
        assert_eq!(
            Error::EmptyPolyomino.to_string(),
            "polyomino cannot be constructed from an empty cell slice"
        );
        assert_eq!(
            Error::DisconnectedPolyomino.to_string(),
            "polyomino cells are not edge-connected"
        );
        assert_eq!(
            Error::IndexOutOfRange(3, 2).to_string(),
            "index 3 is out of range for grid of size 2"
        );
        assert_eq!(
            Error::EmptyOpPolicyValues.to_string(),
            "operation policy received an empty value slice"
        );
        assert_eq!(
            Error::EmptyTuple.to_string(),
            "tuple operation cannot be applied to an empty tuple"
        );
    }

    #[test]
    fn error_display_cage_and_region_variants() {
        use crate::{Cage, Error, Operation, Polyomino, Slot};
        let p = Polyomino::from_cells(&[Cell::new(0, 0)]).unwrap();
        let cage = Cage::new(4, p.clone(), Operation::Given(1));
        assert!(
            Error::CageConflict(cage.clone())
                .to_string()
                .contains("conflicts")
        );
        let cage_msg = Error::SlotNotInPuzzle(Slot::Cage(cage)).to_string();
        assert!(cage_msg.starts_with("cage "));
        assert!(cage_msg.contains("not in this puzzle"));
        let region_msg = Error::SlotNotInPuzzle(Slot::Region(p.clone())).to_string();
        assert!(region_msg.starts_with("region "));
        assert!(region_msg.contains("not in this puzzle"));
        assert!(
            Error::DuplicateSlotPolyomino(p.clone())
                .to_string()
                .contains("duplicate polyomino")
        );
        assert!(
            Error::RegionConflict(p.clone())
                .to_string()
                .contains("conflicts")
        );
        assert!(
            Error::InfeasibleOperation(p, Operation::Given(1))
                .to_string()
                .contains("infeasible")
        );
    }

    #[test]
    fn fill_round_trips_through_json() {
        let fill = Domain::new([1, 3, 5]);
        let json = serde_json::to_string(&fill).unwrap();
        assert_eq!(json, "[1,3,5]");
        let restored: Domain = serde_json::from_str(&json).unwrap();
        assert_eq!(fill, restored);
    }

    #[test]
    fn fill_empty_round_trips_through_json() {
        let fill = Domain::default();
        let json = serde_json::to_string(&fill).unwrap();
        let restored: Domain = serde_json::from_str(&json).unwrap();
        assert_eq!(fill, restored);
    }

    #[test]
    fn fill_deserialize_rejects_out_of_range_values() {
        assert!(serde_json::from_str::<Domain>("[0]").is_err());
        assert!(serde_json::from_str::<Domain>("[10]").is_err());
    }
}
