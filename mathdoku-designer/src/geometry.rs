//! Layout helpers and geometry utilities for the SVG grid.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    unused_results
)]

use std::collections::{BTreeSet, HashSet};

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
///
/// Two cages are adjacent when they share a grid edge. That adjacency graph is
/// the dual of a planar grid subdivision, so by the four-color theorem a proper
/// coloring using at most four colors always exists. `palette_size` is the
/// number of available colors (normally the palette length); the returned color
/// indices stay in `0..palette_size` whenever a proper coloring within that
/// bound exists. Staying within the palette is what stops two adjacent cages
/// from wrapping (via `index % palette_size`) onto the same palette entry.
///
/// Returns `(colors, cage_index)` where `cage_index[r][c]` is the cage index
/// for the cell at `(r, c)`, or `None` if uncovered.
#[must_use]
pub fn assign_colors(
    n: usize,
    cages: &[Vec<Cell>],
    palette_size: usize,
) -> (Vec<usize>, Vec<Vec<Option<usize>>>) {
    let mut cage_index = vec![vec![None::<usize>; n]; n];
    for (i, cells) in cages.iter().enumerate() {
        for &cell in cells {
            cage_index[cell.row][cell.column] = Some(i);
        }
    }

    // Cage adjacency graph: an edge joins two cages sharing a grid edge.
    let mut adjacency = vec![BTreeSet::<usize>::new(); cages.len()];
    for (i, cells) in cages.iter().enumerate() {
        for &cell in cells {
            for nb in neighbors(cell, n) {
                if let Some(j) = cage_index[nb.row][nb.column]
                    && j != i
                {
                    adjacency[i].insert(j);
                    adjacency[j].insert(i);
                }
            }
        }
    }

    (color_cages(&adjacency, palette_size), cage_index)
}

/// Colors the cage adjacency graph so neighbors differ, preferring to stay
/// within `palette_size` colors.
///
/// A fast greedy pass handles the common case; when it already stays within
/// `palette_size` its deterministic, stable result is used directly. Only when
/// greedy overflows the palette does a backtracking search find a proper
/// coloring bounded by `palette_size` — guaranteed to exist for this planar
/// graph — seeded by the greedy colors so it changes as few cages as possible.
/// If no bounded coloring exists (only when `palette_size` is too small to
/// color the graph at all) the greedy result is returned as a fallback.
fn color_cages(adjacency: &[BTreeSet<usize>], palette_size: usize) -> Vec<usize> {
    let greedy = greedy_coloring(adjacency);
    let overflows = palette_size > 0 && greedy.iter().any(|&c| c >= palette_size);
    if !overflows {
        return greedy;
    }
    bounded_coloring(adjacency, palette_size, &greedy).unwrap_or(greedy)
}

/// Greedy graph coloring: each cage takes the lowest color index not already
/// used by one of its neighbors. Unbounded in the number of colors.
fn greedy_coloring(adjacency: &[BTreeSet<usize>]) -> Vec<usize> {
    let mut color = vec![0usize; adjacency.len()];
    for i in 0..adjacency.len() {
        let used: HashSet<usize> = adjacency[i].iter().map(|&j| color[j]).collect();
        let mut k = 0;
        while used.contains(&k) {
            k += 1;
        }
        color[i] = k;
    }
    color
}

/// Backtracking search for a proper coloring using only `0..palette_size`.
///
/// Returns `None` if no such coloring exists. Cages are colored most-constrained
/// (highest-degree) first to keep the search shallow, and each cage tries its
/// `seed` color first so the result diverges from the greedy coloring as little
/// as possible.
fn bounded_coloring(
    adjacency: &[BTreeSet<usize>],
    palette_size: usize,
    seed: &[usize],
) -> Option<Vec<usize>> {
    let mut order: Vec<usize> = (0..adjacency.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(adjacency[i].len()));

    let mut color = vec![usize::MAX; adjacency.len()];
    backtrack(adjacency, palette_size, seed, &order, 0, &mut color).then_some(color)
}

fn backtrack(
    adjacency: &[BTreeSet<usize>],
    palette_size: usize,
    seed: &[usize],
    order: &[usize],
    pos: usize,
    color: &mut [usize],
) -> bool {
    let Some(&v) = order.get(pos) else {
        return true; // every cage is colored
    };
    // Try the cage's greedy color first, then the rest of the palette in turn.
    let preferred = seed[v] % palette_size;
    for offset in 0..palette_size {
        let candidate = (preferred + offset) % palette_size;
        if adjacency[v].iter().any(|&j| color[j] == candidate) {
            continue;
        }
        color[v] = candidate;
        if backtrack(adjacency, palette_size, seed, order, pos + 1, color) {
            return true;
        }
        color[v] = usize::MAX;
    }
    false
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
        let (colors, _) = assign_colors(4, &[], 4);
        assert_eq!(colors, Vec::<usize>::new());
    }

    #[test]
    fn assign_colors_single_cage() {
        let cages = vec![vec![Cell::new(0, 0), Cell::new(0, 1)]];
        let (colors, _) = assign_colors(4, &cages, 4);
        assert_eq!(colors.len(), 1);
    }

    #[test]
    fn assign_colors_adjacent_cages_get_different_colors() {
        let cages = vec![vec![Cell::new(0, 0)], vec![Cell::new(0, 1)]];
        let (colors, _) = assign_colors(4, &cages, 4);
        assert_ne!(colors[0], colors[1]);
    }

    #[test]
    fn assign_colors_non_adjacent_cages_differ_from_their_neighbors() {
        let cages = vec![
            vec![Cell::new(0, 0)],
            vec![Cell::new(0, 1)],
            vec![Cell::new(0, 2)],
        ];
        let (colors, _) = assign_colors(4, &cages, 4);
        assert_ne!(colors[0], colors[1]);
        assert_ne!(colors[1], colors[2]);
    }

    #[test]
    fn assign_colors_four_adjacent_cages_get_distinct_colors() {
        let cages: Vec<Vec<Cell>> = (0..4).map(|c| vec![Cell::new(0, c)]).collect();
        let (colors, _) = assign_colors(4, &cages, 4);
        assert_ne!(colors[0], colors[1]);
        assert_ne!(colors[1], colors[2]);
        assert_ne!(colors[2], colors[3]);
    }

    /// For each pair of grid-adjacent cells in different cages, the two cages
    /// must map to different palette entries, and no color index may fall
    /// outside the palette (which would wrap onto another cage's color).
    fn assert_proper_palette_coloring(
        n: usize,
        colors: &[usize],
        cage_index: &[Vec<Option<usize>>],
        palette_size: usize,
    ) {
        for &c in colors {
            assert!(c < palette_size, "color index {c} is outside the palette");
        }
        for r in 0..n {
            for c in 0..n {
                let Some(a) = cage_index[r][c] else { continue };
                for nb in neighbors(Cell::new(r, c), n) {
                    if let Some(b) = cage_index[nb.row][nb.column]
                        && a != b
                    {
                        assert_ne!(
                            colors[a] % palette_size,
                            colors[b] % palette_size,
                            "adjacent cages {a} and {b} share a palette color"
                        );
                    }
                }
            }
        }
    }

    /// A K4 of cages (four cages all pairwise edge-adjacent) needs all four
    /// palette colors; every adjacent pair must still differ.
    #[test]
    fn assign_colors_k4_cages_are_properly_four_colored() {
        // Layout on a 3×3 grid where A, B, C, D are pairwise adjacent:
        //   A B B
        //   A C D
        //   A D D
        let cages = vec![
            vec![Cell::new(0, 0), Cell::new(1, 0), Cell::new(2, 0)], // A (left column)
            vec![Cell::new(0, 1), Cell::new(0, 2)],                  // B (top)
            vec![Cell::new(1, 1)],                                   // C (center)
            vec![Cell::new(1, 2), Cell::new(2, 1), Cell::new(2, 2)], // D (bottom-right)
        ];
        let (colors, cage_index) = assign_colors(3, &cages, 4);
        assert_proper_palette_coloring(3, &colors, &cage_index, 4);
    }

    /// Regression for the "singleton color not distinct" bug: when greedy
    /// coloring overflows the palette, the bounded search must repair it into a
    /// proper coloring whose indices all stay within the palette. The adjacency
    /// here is K4 (cages 0–3) plus a pendant (cage 4); greedy assigns cage 3 the
    /// fifth color index, which `% 4` would collapse onto a neighbor.
    #[test]
    fn color_cages_repairs_greedy_overflow_within_palette() {
        let adjacency = vec![
            BTreeSet::from([1, 2, 3]),
            BTreeSet::from([0, 2, 3]),
            BTreeSet::from([0, 1, 3]),
            BTreeSet::from([0, 1, 2, 4]),
            BTreeSet::from([3]),
        ];

        // Greedy overflows: cage 3 is forced to color index 4.
        let greedy = greedy_coloring(&adjacency);
        assert!(
            greedy.iter().any(|&c| c >= 4),
            "expected greedy to overflow a 4-color palette, got {greedy:?}"
        );

        // The bounded coloring fits in four colors and stays proper.
        let colors = color_cages(&adjacency, 4);
        for &c in &colors {
            assert!(c < 4, "color index {c} is outside the palette");
        }
        for (i, neighbors) in adjacency.iter().enumerate() {
            for &j in neighbors {
                assert_ne!(colors[i], colors[j], "adjacent cages {i} and {j} match");
            }
        }
    }
}
