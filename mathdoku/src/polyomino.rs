//! A [`Polyomino`]: a contiguous, edge-connected region of grid cells.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Cell, Error};

/// A contiguous region of edge-connected [`Cell`]s.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct Polyomino(BTreeSet<Cell>);

impl Polyomino {
    /// Constructs a polyomino from a set of cells.
    ///
    /// # Errors
    /// Returns [`Error::EmptyPolyomino`] if `cells` is empty, or
    /// [`Error::DisconnectedPolyomino`] if `cells` is not edge-connected.
    fn new(cells: BTreeSet<Cell>) -> Result<Self, Error> {
        if cells.is_empty() {
            return Err(Error::EmptyPolyomino);
        }
        if !is_edge_connected_component(&cells) {
            return Err(Error::DisconnectedPolyomino);
        }
        Ok(Self(cells))
    }

    /// Constructs a polyomino from a slice of cells.
    ///
    /// # Errors
    /// Returns [`Error::EmptyPolyomino`] if `cells` is empty, or
    /// [`Error::DisconnectedPolyomino`] if `cells` is not edge-connected.
    pub fn from_cells(cells: &[Cell]) -> Result<Self, Error> {
        Self::new(cells.iter().copied().collect())
    }

    /// Returns the number of cells in this polyomino.
    ///
    /// Always at least 1: a polyomino cannot be empty by construction.
    #[allow(clippy::len_without_is_empty)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// The cells of the polyomino in row-major order.
    #[must_use]
    pub fn cells(&self) -> Vec<Cell> {
        self.0.iter().copied().collect()
    }

    /// The cells grouped by row, in row-major order. Each inner vec is sorted by column.
    #[must_use]
    pub fn rows(&self) -> Vec<Vec<Cell>> {
        let mut rows: Vec<Vec<Cell>> = Vec::new();
        for &cell in &self.0 {
            match rows.last_mut() {
                Some(row) if row[0].row == cell.row => row.push(cell),
                _ => rows.push(vec![cell]),
            }
        }
        rows
    }

    /// The cells grouped by column, in column-major order. Each inner vec is sorted by row.
    #[must_use]
    pub fn columns(&self) -> Vec<Vec<Cell>> {
        let mut cols: BTreeMap<usize, Vec<Cell>> = BTreeMap::new();
        for &cell in &self.0 {
            cols.entry(cell.column).or_default().push(cell);
        }
        cols.into_values().collect()
    }

    /// Returns a new polyomino with `cell` added.
    ///
    /// Idempotent: if `cell` is already present, returns an equivalent
    /// polyomino.
    ///
    /// # Errors
    /// Returns [`Error::DisconnectedPolyomino`] if adding `cell` would make the
    /// polyomino disconnected.
    pub fn insert(&self, cell: Cell) -> Result<Self, Error> {
        let mut cells = self.0.clone();
        let _ = cells.insert(cell);
        Self::new(cells)
    }
    /// Returns a new polyomino with `cell` removed.
    ///
    /// If `cell` is not present in the polyomino, returns an equivalent polyomino.
    ///
    /// # Errors
    /// Returns [`Error::RemovalWouldEmptyPolyomino`] if `cell` is the only cell,
    /// or [`Error::WouldDisconnect`] if removing `cell` would leave the remaining
    /// cells edge-disconnected.
    pub fn remove(&self, cell: Cell) -> Result<Self, Error> {
        let cells: BTreeSet<Cell> = self.0.iter().copied().filter(|c| *c != cell).collect();
        if cells.is_empty() {
            return Err(Error::RemovalWouldEmptyPolyomino(cell));
        }
        if !is_edge_connected_component(&cells) {
            return Err(Error::WouldDisconnect(cell));
        }
        Ok(Self(cells))
    }

    /// Returns `true` if this polyomino shares at least one cell with `other`.
    #[must_use]
    pub fn intersects(&self, other: &Self) -> bool {
        self.0.intersection(&other.0).next().is_some()
    }
}

/// Do `cells` form a contiguous edge-connected component?
/// Two `cell`s are edge-connected if they share a common edge.
fn is_edge_connected_component(cells: &BTreeSet<Cell>) -> bool {
    let Some(&start) = cells.first() else {
        return true;
    };
    let mut visited = HashSet::new();
    let mut stack = vec![start];
    while let Some(cell) = stack.pop() {
        if visited.insert(cell) {
            for neighbor in cell.neighbors_4() {
                if cells.contains(&neighbor) {
                    stack.push(neighbor);
                }
            }
        }
    }
    visited.len() == cells.len()
}

impl Serialize for Polyomino {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.iter())
    }
}

impl<'de> Deserialize<'de> for Polyomino {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let cells = Vec::<Cell>::deserialize(d)?;
        Self::from_cells(&cells).map_err(DeError::custom)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{from_str, to_string};

    use super::*;
    use crate::test_utils::{
        c00, c01, c02, c10, c11, cells, col_pair, l_shape, pair, row3, singleton,
    };

    fn btree(positions: &[(usize, usize)]) -> BTreeSet<Cell> {
        cells(positions).into_iter().collect()
    }

    impl Polyomino {
        fn contains(&self, cell: Cell) -> bool {
            self.0.contains(&cell)
        }
    }

    // --- is_edge_adjacent ---

    #[test]
    fn is_edge_adjacent_empty_is_true() {
        assert!(is_edge_connected_component(&BTreeSet::new()));
    }

    #[test]
    fn is_edge_adjacent_single_cell_is_true() {
        assert!(is_edge_connected_component(&btree(&[(0, 0)])));
    }

    #[test]
    fn is_edge_adjacent_horizontal_pair_is_true() {
        assert!(is_edge_connected_component(&btree(&[(0, 0), (0, 1)])));
    }

    #[test]
    fn is_edge_adjacent_vertical_pair_is_true() {
        assert!(is_edge_connected_component(&btree(&[(0, 0), (1, 0)])));
    }

    #[test]
    fn is_edge_adjacent_diagonal_pair_is_false() {
        assert!(!is_edge_connected_component(&btree(&[(0, 0), (1, 1)])));
    }

    #[test]
    fn is_edge_adjacent_l_shape_is_true() {
        assert!(is_edge_connected_component(&btree(&[
            (0, 0),
            (1, 0),
            (1, 1)
        ])));
    }

    #[test]
    fn is_edge_adjacent_disconnected_is_false() {
        assert!(!is_edge_connected_component(&btree(&[(0, 0), (0, 2)])));
    }

    // --- Polyomino ---

    #[test]
    fn polyomino_cells_are_sorted() {
        let r = Polyomino::from_cells(&[c10(), c00(), c01()]).unwrap();
        itertools::assert_equal(r.cells(), [c00(), c01(), c10()]);
    }

    #[test]
    fn polyomino_len_matches_cell_count() {
        assert_eq!(row3().len(), 3);
    }

    #[test]
    fn polyomino_new_empty_returns_err() {
        assert!(matches!(
            Polyomino::from_cells(&[]),
            Err(Error::EmptyPolyomino)
        ));
    }

    #[test]
    fn polyomino_new_disconnected_returns_err() {
        assert!(matches!(
            Polyomino::from_cells(&[c00(), c02()]),
            Err(Error::DisconnectedPolyomino)
        ));
    }

    #[test]
    fn polyomino_new_distinguishes_empty_from_disconnected() {
        // #45: empty and disconnected inputs must report distinct errors.
        assert!(matches!(
            Polyomino::from_cells(&[]),
            Err(Error::EmptyPolyomino)
        ));
        assert!(matches!(
            Polyomino::from_cells(&[c00(), c11()]),
            Err(Error::DisconnectedPolyomino)
        ));
    }

    #[test]
    fn polyomino_new_rejects_diagonal_only_inputs() {
        // #46: a diagonal-only set of cells is not edge-connected and must be rejected.
        assert!(matches!(
            Polyomino::from_cells(&[c00(), c11(), Cell::new(2, 2)]),
            Err(Error::DisconnectedPolyomino)
        ));
    }

    // --- rows / columns ---

    #[test]
    fn rows_singleton() {
        assert_eq!(singleton().rows(), vec![vec![c00()]]);
    }

    #[test]
    fn rows_horizontal_pair() {
        assert_eq!(pair().rows(), vec![vec![c00(), c01()]]);
    }

    #[test]
    fn rows_l_shape() {
        // l_shape: (0,0), (1,0), (1,1)
        assert_eq!(l_shape().rows(), vec![vec![c00()], vec![c10(), c11()]]);
    }

    #[test]
    fn columns_singleton() {
        assert_eq!(singleton().columns(), vec![vec![c00()]]);
    }

    #[test]
    fn columns_horizontal_pair() {
        assert_eq!(pair().columns(), vec![vec![c00()], vec![c01()]]);
    }

    #[test]
    fn columns_l_shape() {
        // l_shape: (0,0), (1,0), (1,1) — col 0 has (0,0),(1,0); col 1 has (1,1)
        assert_eq!(l_shape().columns(), vec![vec![c00(), c10()], vec![c11()]]);
    }

    #[test]
    fn insert_adds_adjacent_cell() {
        let p2 = singleton().insert(c01()).unwrap();
        assert!(p2.cells().contains(&c01()));
        assert_eq!(p2.len(), 2);
    }

    #[test]
    fn insert_is_idempotent() {
        assert_eq!(pair().insert(c00()).unwrap().len(), 2);
    }

    #[test]
    fn insert_disconnected_returns_err() {
        assert!(matches!(
            singleton().insert(c02()),
            Err(Error::DisconnectedPolyomino)
        ));
    }

    // --- remove ---

    #[test]
    fn remove_shrinks_polyomino() {
        let p = pair().remove(c01()).unwrap();
        assert_eq!(p.len(), 1);
        assert!(p.contains(c00()));
        assert!(!p.contains(c01()));
    }

    #[test]
    fn remove_absent_cell_is_idempotent() {
        // Removing a cell not in the polyomino returns an equivalent polyomino.
        let p = pair().remove(Cell::new(9, 9)).unwrap();
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn remove_last_cell_returns_err() {
        assert!(matches!(
            singleton().remove(c00()),
            Err(Error::RemovalWouldEmptyPolyomino(_))
        ));
    }

    #[test]
    fn remove_disconnecting_cell_returns_err() {
        // row3: (0,0)-(0,1)-(0,2); removing the middle disconnects the ends.
        assert!(matches!(
            row3().remove(c01()),
            Err(Error::WouldDisconnect(_))
        ));
    }

    // --- intersects ---

    #[test]
    fn intersects_overlapping_polyominoes() {
        assert!(pair().intersects(&singleton()));
    }

    #[test]
    fn intersects_disjoint_polyominoes() {
        // singleton at (0,0); col_pair at (0,0),(1,0) — they share (0,0)
        // Use a truly disjoint pair instead.
        let a = Polyomino::from_cells(&cells(&[(0, 0)])).unwrap();
        let b = Polyomino::from_cells(&cells(&[(1, 1)])).unwrap();
        // (1,1) is diagonally adjacent only — no shared cell.
        assert!(!a.intersects(&b));
    }

    #[test]
    fn intersects_is_symmetric() {
        let a = pair();
        let b = col_pair();
        assert_eq!(a.intersects(&b), b.intersects(&a));
    }

    // --- contains ---

    #[test]
    fn contains_present_cell() {
        assert!(pair().contains(c00()));
        assert!(pair().contains(c01()));
    }

    #[test]
    fn contains_absent_cell() {
        assert!(!pair().contains(c10()));
    }

    // --- serde round-trip ---

    #[test]
    fn polyomino_round_trips_through_json() {
        let p = l_shape();
        let json = to_string(&p).unwrap();
        let restored: Polyomino = from_str(&json).unwrap();
        assert_eq!(p, restored);
    }

    #[test]
    fn polyomino_deserialize_empty_returns_err() {
        assert!(from_str::<Polyomino>("[]").is_err());
    }

    #[test]
    fn polyomino_deserialize_disconnected_returns_err() {
        // (0,0) and (1,1) are diagonal only — not edge-connected.
        assert!(from_str::<Polyomino>(r#"[{"row":0,"column":0},{"row":1,"column":1}]"#).is_err());
    }
}
