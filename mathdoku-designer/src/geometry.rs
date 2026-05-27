//! Layout helpers and geometry utilities for the SVG grid.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    unused_results
)]

use std::collections::HashSet;

use mathdoku::Cell;

pub const MARGIN: f64 = 14.0;
pub const THICK: f64 = 2.2;
pub const THIN: f64 = 0.5;

/// Returns the cell side length in SVG units for an *n*×*n* grid.
#[must_use]
pub fn cell_size(n: usize) -> f64 {
    let viewport = 600.0_f64;
    2.0f64.mul_add(-MARGIN, viewport) / (n as f64).max(1.0)
}

/// Returns the cage-label font size for a given cell side length.
#[must_use]
pub fn op_font(cell: f64) -> f64 {
    (cell * 0.16).max(10.0)
}

/// Returns the SVG `(x, y)` top-left corner of the cell at `(row, col)`.
#[must_use]
pub const fn origin(cell: f64, row: usize, col: usize) -> (f64, f64) {
    (
        (col as f64).mul_add(cell, MARGIN),
        (row as f64).mul_add(cell, MARGIN),
    )
}

/// Returns the anchor cell of a cage: the topmost cell in the leftmost column.
#[must_use]
pub fn anchor(cells: &[Cell]) -> Cell {
    cells
        .iter()
        .copied()
        .min_by_key(|c| (c.column, c.row))
        .unwrap_or(Cell::new(0, 0))
}

fn neighbors(cell: Cell, n: usize) -> impl Iterator<Item = Cell> {
    cell.neighbors_4()
        .filter(move |c| c.row < n && c.column < n)
}

/// Assigns palette colors to cages so adjacent cages get different colors.
/// Returns `(colors, cage_index)` where `cage_index[r][c]` is the cage index
/// for the cell at `(r, c)`, or `None` if uncovered.
#[must_use]
pub fn assign_colors(n: usize, cages: &[Vec<Cell>]) -> (Vec<usize>, Vec<Vec<Option<usize>>>) {
    let mut cage_index = vec![vec![None::<usize>; n]; n];
    for (i, cells) in cages.iter().enumerate() {
        for &cell in cells {
            cage_index[cell.row][cell.column] = Some(i);
        }
    }
    let mut color = vec![0usize; cages.len()];
    for (i, cells) in cages.iter().enumerate() {
        let mut used = HashSet::new();
        for &cell in cells {
            for nb in neighbors(cell, n) {
                if let Some(j) = cage_index[nb.row][nb.column]
                    && j != i
                {
                    used.insert(color[j]);
                }
            }
        }
        let mut k = 0;
        while used.contains(&k) {
            k += 1;
        }
        color[i] = k;
    }
    (color, cage_index)
}

#[must_use]
pub const fn is_thick(a: Option<usize>, b: Option<usize>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => x != y, // boundary between two different cages
        (None, None) => false,        // boundary between two uncaged cells
        _ => true,                    // boundary between a caged and uncaged cell
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_size_divides_viewport_evenly() {
        let c = cell_size(4);
        assert!((c - 2.0f64.mul_add(-MARGIN, 600.0) / 4.0).abs() < 1e-10);
    }

    #[test]
    fn cell_size_never_zero_for_n_zero() {
        assert!(cell_size(0) > 0.0);
    }

    #[test]
    fn cell_size_decreases_with_larger_n() {
        assert!(cell_size(4) > cell_size(9));
    }

    #[test]
    fn origin_row_zero_col_zero_is_margin() {
        let c = cell_size(4);
        assert_eq!(origin(c, 0, 0), (MARGIN, MARGIN));
    }

    #[test]
    fn origin_advances_by_cell_size() {
        let c = cell_size(4);
        let (x0, y0) = origin(c, 0, 0);
        let (x1, y1) = origin(c, 1, 1);
        assert!((x1 - x0 - c).abs() < 1e-10);
        assert!((y1 - y0 - c).abs() < 1e-10);
    }

    #[test]
    fn op_font_scales_with_cell() {
        let large = op_font(100.0);
        let small = op_font(50.0);
        assert!(large > small);
    }

    #[test]
    fn op_font_minimum_is_ten() {
        assert!((op_font(0.0) - 10.0).abs() < 1e-10);
    }

    #[test]
    fn anchor_single_cell() {
        assert_eq!(anchor(&[Cell::new(3, 2)]), Cell::new(3, 2));
    }

    #[test]
    fn anchor_picks_leftmost_then_topmost() {
        assert_eq!(anchor(&[Cell::new(1, 0), Cell::new(0, 1)]), Cell::new(1, 0));
    }

    #[test]
    fn anchor_tiebreaks_by_row() {
        assert_eq!(anchor(&[Cell::new(2, 1), Cell::new(0, 1)]), Cell::new(0, 1));
    }

    #[test]
    fn anchor_empty_returns_default() {
        assert_eq!(anchor(&[]), Cell::new(0, 0));
    }

    #[test]
    fn is_thick_both_same_cage_is_thin() {
        assert!(!is_thick(Some(0), Some(0)));
    }

    #[test]
    fn is_thick_different_cages_is_thick() {
        assert!(is_thick(Some(0), Some(1)));
    }

    #[test]
    fn is_thick_one_none_is_thick() {
        assert!(is_thick(Some(0), None));
        assert!(is_thick(None, Some(0)));
    }

    #[test]
    fn is_thick_both_none_is_thin() {
        assert!(!is_thick(None, None));
    }

    #[test]
    fn assign_colors_empty_cages() {
        let (colors, _) = assign_colors(4, &[]);
        assert_eq!(colors, Vec::<usize>::new());
    }

    #[test]
    fn assign_colors_single_cage() {
        let cages = vec![vec![Cell::new(0, 0), Cell::new(0, 1)]];
        let (colors, _) = assign_colors(4, &cages);
        assert_eq!(colors.len(), 1);
    }

    #[test]
    fn assign_colors_adjacent_cages_get_different_colors() {
        let cages = vec![vec![Cell::new(0, 0)], vec![Cell::new(0, 1)]];
        let (colors, _) = assign_colors(4, &cages);
        assert_ne!(colors[0], colors[1]);
    }

    #[test]
    fn assign_colors_non_adjacent_cages_differ_from_their_neighbors() {
        let cages = vec![
            vec![Cell::new(0, 0)],
            vec![Cell::new(0, 1)],
            vec![Cell::new(0, 2)],
        ];
        let (colors, _) = assign_colors(4, &cages);
        assert_ne!(colors[0], colors[1]);
        assert_ne!(colors[1], colors[2]);
    }

    #[test]
    fn assign_colors_four_adjacent_cages_get_distinct_colors() {
        let cages: Vec<Vec<Cell>> = (0..4).map(|c| vec![Cell::new(0, c)]).collect();
        let (colors, _) = assign_colors(4, &cages);
        assert_ne!(colors[0], colors[1]);
        assert_ne!(colors[1], colors[2]);
        assert_ne!(colors[2], colors[3]);
    }
}
