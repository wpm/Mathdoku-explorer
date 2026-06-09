use crate::Error;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

/// A grid position identified by `(row, column)`, both 1-indexed.
#[derive(Ord, Eq, PartialEq, Hash, PartialOrd, Copy, Clone, Debug, Serialize, Deserialize)]
pub struct Cell(pub usize, pub usize);

impl Cell {
    /// Creates a cell from 0-indexed `(row, column)` coordinates.
    ///
    /// Converts to 1-indexed storage used internally.
    #[must_use]
    pub const fn new(row: usize, col: usize) -> Self {
        Self(row + 1, col + 1)
    }

    /// Returns the 0-indexed row of this cell.
    #[must_use]
    pub const fn row(self) -> usize {
        self.0 - 1
    }

    /// Returns the 0-indexed column of this cell.
    #[must_use]
    pub const fn column(self) -> usize {
        self.1 - 1
    }

    /// Returns the 4-connected neighbors of this cell (unbounded, may include cells with row or
    /// column equal to `usize::MAX` if the cell is at row/column 0).
    #[must_use]
    pub fn neighbors_4(self) -> Vec<Self> {
        let r = self.row();
        let c = self.column();
        let mut result = Vec::with_capacity(4);
        if r > 0 {
            result.push(Self::new(r - 1, c));
        }
        result.push(Self::new(r + 1, c));
        if c > 0 {
            result.push(Self::new(r, c - 1));
        }
        result.push(Self::new(r, c + 1));
        result
    }
}

impl fmt::Display for Cell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.0, self.1)
    }
}

/// A set of edge-adjacent cells.
#[derive(Eq, PartialEq, Ord, PartialOrd, Clone, Hash, Debug, Serialize, Deserialize)]
pub struct Polyomino(BTreeSet<Cell>);

impl Polyomino {
    /// Constructs a polyomino from `cells`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DisconnectedPolyomino`] if the cells are empty or not edge-connected.
    pub fn from(cells: impl IntoIterator<Item = Cell>) -> Result<Self, Error> {
        let cells: Vec<Cell> = cells.into_iter().collect();
        if is_edge_adjacent(&cells) {
            Ok(Self(BTreeSet::from_iter(cells)))
        } else {
            Err(Error::DisconnectedPolyomino)
        }
    }

    /// Returns the number of cells in this polyomino.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if this polyomino contains no cells.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns `true` if `cell` is part of this polyomino.
    #[must_use]
    pub fn contains(&self, cell: &Cell) -> bool {
        self.0.contains(cell)
    }

    /// Returns `true` if this polyomino shares no cells with `other`.
    #[must_use]
    pub fn is_disjoint(&self, other: &Self) -> bool {
        self.0.is_disjoint(&other.0)
    }

    /// Returns an iterator over the cells of this polyomino in sorted order.
    pub fn iter(&self) -> impl Iterator<Item = &Cell> {
        self.0.iter()
    }

    /// Alias for [`Polyomino::from`] accepting a slice.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DisconnectedPolyomino`] if the cells are empty or not edge-connected.
    pub fn from_cells(cells: &[Cell]) -> Result<Self, Error> {
        Self::from(cells.iter().copied())
    }

    /// Returns the cells of this polyomino as a `Vec`.
    #[must_use]
    pub fn cells(&self) -> Vec<Cell> {
        self.0.iter().copied().collect()
    }

    /// Returns a new polyomino with `cell` added.
    ///
    /// If `cell` is already in the polyomino, returns a clone unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`Error::DisconnectedPolyomino`] if the result is not edge-connected.
    pub fn insert(&self, cell: Cell) -> Result<Self, Error> {
        if self.contains(&cell) {
            return Ok(self.clone());
        }
        let cells: Vec<Cell> = self
            .0
            .iter()
            .copied()
            .chain(std::iter::once(cell))
            .collect();
        Self::from(cells)
    }
}

/// Returns `true` if the cells form an edge-connected component.
///
/// Uses DFS from the first cell. When checking neighbours, only looks right
/// (col+1) and down (row+1) while iterating — sufficient because the set is
/// sorted row-major and back-edges (left/up) are discovered from the other end.
fn is_edge_adjacent(cells: &[Cell]) -> bool {
    if cells.is_empty() {
        return false;
    }
    let mut visited: BTreeSet<Cell> = BTreeSet::new();
    let mut stack: Vec<Cell> = vec![cells[0]];
    while let Some(cell) = stack.pop() {
        if visited.insert(cell) {
            let Cell(r, c) = cell;
            for neighbor in [
                Cell(r, c + 1),
                Cell(r + 1, c),
                Cell(r, c.wrapping_sub(1)),
                Cell(r.wrapping_sub(1), c),
            ] {
                if cells.contains(&neighbor) {
                    stack.push(neighbor);
                }
            }
        }
    }
    visited.len() == cells.len()
}

impl IntoIterator for Polyomino {
    type Item = Cell;
    type IntoIter = std::collections::btree_set::IntoIter<Cell>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use crate::Error;
    use crate::polyomino::{Cell, Polyomino};

    #[test]
    fn cell_display_formats_as_row_column() {
        assert_eq!(Cell(2, 3).to_string(), "(2, 3)");
    }

    #[test]
    fn polyomino_single_cell_is_connected() {
        assert!(Polyomino::from([Cell(1, 1)]).is_ok());
    }

    #[test]
    fn polyomino_horizontal_pair_is_connected() {
        assert!(Polyomino::from([Cell(1, 1), Cell(1, 2)]).is_ok());
    }

    #[test]
    fn polyomino_vertical_pair_is_connected() {
        assert!(Polyomino::from([Cell(1, 1), Cell(2, 1)]).is_ok());
    }

    #[test]
    fn polyomino_l_shape_is_connected() {
        assert!(Polyomino::from([Cell(1, 1), Cell(1, 2), Cell(2, 1)]).is_ok());
    }

    #[test]
    fn polyomino_empty_is_disconnected() {
        assert!(matches!(
            Polyomino::from([]),
            Err(Error::DisconnectedPolyomino)
        ));
    }

    #[test]
    fn polyomino_diagonal_pair_is_disconnected() {
        assert!(matches!(
            Polyomino::from([Cell(1, 1), Cell(2, 2)]),
            Err(Error::DisconnectedPolyomino)
        ));
    }

    #[test]
    fn polyomino_two_separate_pairs_is_disconnected() {
        assert!(matches!(
            Polyomino::from([Cell(1, 1), Cell(1, 2), Cell(3, 3), Cell(3, 4)]),
            Err(Error::DisconnectedPolyomino)
        ));
    }

    #[test]
    fn is_disjoint_no_overlap_returns_true() {
        let a = Polyomino::from([Cell(1, 1), Cell(1, 2)]).unwrap();
        let b = Polyomino::from([Cell(2, 1), Cell(2, 2)]).unwrap();
        assert!(a.is_disjoint(&b));
    }

    #[test]
    fn is_disjoint_partial_overlap_returns_false() {
        let a = Polyomino::from([Cell(1, 1), Cell(1, 2)]).unwrap();
        let b = Polyomino::from([Cell(1, 2), Cell(1, 3)]).unwrap();
        assert!(!a.is_disjoint(&b));
    }

    #[test]
    fn is_disjoint_identical_returns_false() {
        let a = Polyomino::from([Cell(1, 1)]).unwrap();
        let b = Polyomino::from([Cell(1, 1)]).unwrap();
        assert!(!a.is_disjoint(&b));
    }

    #[test]
    fn is_disjoint_is_symmetric() {
        let a = Polyomino::from([Cell(1, 1), Cell(1, 2)]).unwrap();
        let b = Polyomino::from([Cell(2, 1), Cell(2, 2)]).unwrap();
        assert_eq!(a.is_disjoint(&b), b.is_disjoint(&a));
    }

    #[test]
    fn polyomino_into_iter_yields_cells_in_order() {
        let p = Polyomino::from([Cell(2, 1), Cell(1, 2), Cell(1, 1)]).unwrap();
        let cells: Vec<Cell> = p.into_iter().collect();
        assert_eq!(cells, vec![Cell(1, 1), Cell(1, 2), Cell(2, 1)]);
    }

    #[test]
    fn polyomino_into_iter_singleton() {
        let p = Polyomino::from([Cell(3, 4)]).unwrap();
        let cells: Vec<Cell> = p.into_iter().collect();
        assert_eq!(cells, vec![Cell(3, 4)]);
    }
}
