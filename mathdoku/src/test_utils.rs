//! Shared test fixtures for building cells and polyominoes.

#![allow(clippy::missing_panics_doc)]

use crate::Cell;
use crate::polyomino::Polyomino;

/// Builds [`Cell`]s from `(row, column)` pairs.
pub fn cells(positions: &[(usize, usize)]) -> Vec<Cell> {
    positions.iter().map(|&(r, c)| Cell::new(r, c)).collect()
}

pub fn c00() -> Cell {
    Cell::new(0, 0)
}
pub fn c01() -> Cell {
    Cell::new(0, 1)
}
pub fn c02() -> Cell {
    Cell::new(0, 2)
}
pub fn c10() -> Cell {
    Cell::new(1, 0)
}
pub fn c11() -> Cell {
    Cell::new(1, 1)
}

pub fn singleton() -> Polyomino {
    Polyomino::from_cells(&cells(&[(0, 0)])).unwrap()
}

pub fn pair() -> Polyomino {
    Polyomino::from_cells(&cells(&[(0, 0), (0, 1)])).unwrap()
}

pub fn col_pair() -> Polyomino {
    Polyomino::from_cells(&cells(&[(0, 0), (1, 0)])).unwrap()
}

pub fn row3() -> Polyomino {
    Polyomino::from_cells(&cells(&[(0, 0), (0, 1), (0, 2)])).unwrap()
}

pub fn l_shape() -> Polyomino {
    Polyomino::from_cells(&cells(&[(0, 0), (1, 0), (1, 1)])).unwrap()
}
