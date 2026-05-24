//! Puzzle component: SVG root, layout, interaction, and subcomponent orchestration.
//!
//! # Interaction modes
//!
//! The puzzle is always in one of two **interaction modes**:
//!
//! ## Cell Mode
//! The fundamental unit of selection is an individual grid cell.
//! - One cell is selected at a time, shown with a heavier outline.
//! - Arrow keys move the selection; movement stops at the puzzle boundary.
//! - Tab or Shift-Tab switches to Slot Mode, selecting the slot that contains
//!   the current cell.
//!
//! ## Slot Mode
//! The fundamental unit of selection is a cage or region slot (a polyomino).
//! - One slot is selected at a time; all cells of that slot receive a heavier
//!   outline.
//! - Tab advances to the next slot in polyomino order (wrapping around).
//! - Shift-Tab moves to the previous slot (wrapping around).
//! - An arrow key switches back to Cell Mode, placing the selection at the
//!   anchor cell of the current slot and then moving one step in that direction.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]

use mathdoku::Puzzle as KenkenPuzzle;
use mathdoku_designer_shared::{Mode, ViewState};
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use super::cage::Cage;
use super::cage_stats::CageStats;
use super::cell::Cell;
use super::region::Region;
use super::selection::SelectionOverlay;
use super::solution_count::SolutionCount;

// ---- visual constants (pub for SelectionOverlay) ----
pub const MARGIN: f64 = 14.0;
const THICK: f64 = 2.2;
const THIN: f64 = 0.5;
const OP_INSET: f64 = 4.0;

const BG: &str = "#f4efe6";
const INK: &str = "#26221b";
const LINE: &str = "#b9ad93";

const CAGE_PALETTE: [&str; 4] = ["#cfe4f2", "#d7ecd5", "#f7ecc6", "#f6d9d3"];

// ---- context ----

/// Puzzle and its cage list, shared via context for on-demand viable-count queries.
///
/// The `Mutex` is needed only to satisfy `Send + Sync` for `provide_context`; on
/// single-threaded WASM there is never actual contention.
#[derive(Clone)]
pub struct PuzzleRef(std::sync::Arc<PuzzleRefInner>);

struct PuzzleRefInner {
    puzzle: std::sync::Mutex<KenkenPuzzle>,
    /// Cage for each slot; `None` for region slots.
    cages: Vec<Option<mathdoku::Cage>>,
}

impl PuzzleRef {
    fn new(puzzle: KenkenPuzzle, cages: Vec<Option<mathdoku::Cage>>) -> Self {
        Self(std::sync::Arc::new(PuzzleRefInner {
            puzzle: std::sync::Mutex::new(puzzle),
            cages,
        }))
    }

    /// Returns the number of solutions, or `None` if the puzzle is incomplete.
    ///
    /// The result is cached inside the library after the first call. The first
    /// call may block (DFS search), so callers should use `spawn_local`.
    pub fn solution_count(&self) -> Option<usize> {
        self.0
            .puzzle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .solution_count()
    }

    /// Returns `(multisets, tuples)` for `slot_idx`, or `None` for a region slot.
    pub fn viable_counts(&self, slot_idx: usize) -> Option<(usize, usize)> {
        let cage = self.0.cages.get(slot_idx)?.as_ref()?;
        let puzzle = self
            .0
            .puzzle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Some((
            puzzle.viable_multiset_count(cage),
            puzzle.viable_tuple_count(cage),
        ))
    }
}

/// An entry in the undo queue. Both provisional-region edits and puzzle
/// mutations go into the same queue so a single Cmd-Z walks back all changes.
#[derive(Clone)]
#[allow(dead_code)]
pub enum UndoEntry {
    /// One Shift+Arrow step that grew the provisional region by one cell.
    /// Undo: pop the last cell from the provisional region.
    AddProvisionalCell,
    /// A region was committed (Enter). Undo: remove the region from the puzzle
    /// (via `remove_region` command) and restore the puzzle to `old_puzzle`.
    CommitRegion {
        /// The cells that form the committed region (for the remove_region call).
        cells: Vec<(usize, usize)>,
    },
}

/// Shared grid state provided to all sub-components via context.
#[derive(Clone)]
pub struct GridContext {
    pub mode: RwSignal<Mode>,
    pub selected_cell: RwSignal<(usize, usize)>,
    pub selected_slot: RwSignal<usize>,
    /// Cells for each slot, in slot order.
    pub slot_cells: Vec<Vec<(usize, usize)>>,
    /// Slot index for each cell, indexed by [row][col].
    pub cell_slot: Vec<Vec<Option<usize>>>,
    /// Puzzle reference for on-demand viable-count queries.
    pub puzzle_ref: PuzzleRef,
    /// Cell size in SVG units.
    pub cell: f64,
    /// Cells forming the provisional region being drawn (empty = none).
    pub provisional_region: RwSignal<Vec<(usize, usize)>>,
    /// Undo queue: most-recent entry last.
    #[allow(dead_code)]
    pub undo_stack: RwSignal<Vec<UndoEntry>>,
}

// ---- layout helpers ----

/// Returns the cell side length in SVG units for an *n*×*n* grid.
pub fn cell_size(n: usize) -> f64 {
    let viewport = 600.0_f64;
    2.0f64.mul_add(-MARGIN, viewport) / (n as f64).max(1.0)
}

/// Returns the cage-label font size for a given cell side length.
pub fn op_font(cell: f64) -> f64 {
    (cell * 0.16).max(10.0)
}

/// Returns the SVG `(x, y)` top-left corner of the cell at `(row, col)`.
pub const fn origin(cell: f64, row: usize, col: usize) -> (f64, f64) {
    (
        (col as f64).mul_add(cell, MARGIN),
        (row as f64).mul_add(cell, MARGIN),
    )
}

/// Returns the anchor cell of a slot: the topmost cell in the leftmost column.
pub fn anchor(cells: &[(usize, usize)]) -> (usize, usize) {
    cells
        .iter()
        .copied()
        .min_by_key(|&(r, c)| (c, r))
        .unwrap_or((0, 0))
}

fn neighbors(r: usize, c: usize, n: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if r > 0 {
        out.push((r - 1, c));
    }
    if c > 0 {
        out.push((r, c - 1));
    }
    if r + 1 < n {
        out.push((r + 1, c));
    }
    if c + 1 < n {
        out.push((r, c + 1));
    }
    out
}

fn assign_colors(n: usize, slots: &[Vec<(usize, usize)>]) -> (Vec<usize>, Vec<Vec<Option<usize>>>) {
    let mut cell_slot = vec![vec![None::<usize>; n]; n];
    for (i, cells) in slots.iter().enumerate() {
        for &(r, c) in cells {
            cell_slot[r][c] = Some(i);
        }
    }
    let mut color = vec![0usize; slots.len()];
    for (i, cells) in slots.iter().enumerate() {
        let mut used = std::collections::HashSet::new();
        for &(r, c) in cells {
            for (nr, nc) in neighbors(r, c, n) {
                if let Some(j) = cell_slot[nr][nc] {
                    if j != i {
                        used.insert(color[j]);
                    }
                }
            }
        }
        let mut k = 0;
        while used.contains(&k) {
            k += 1;
        }
        color[i] = k;
    }
    (color, cell_slot)
}

const fn is_thick(a: Option<usize>, b: Option<usize>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => x != y,
        _ => true,
    }
}

// ---- Tauri IPC ----

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

// ---- Puzzle component ----

type SlotList = Vec<(Vec<(usize, usize)>, Option<mathdoku::Cage>)>;

#[component]
#[allow(clippy::needless_pass_by_value, clippy::too_many_lines)]
pub fn Puzzle(
    puzzle: KenkenPuzzle,
    initial_view: ViewState,
    on_puzzle_change: Callback<(KenkenPuzzle, ViewState)>,
) -> impl IntoView {
    let n = puzzle.n();
    let cell = cell_size(n);
    let op_f = op_font(cell);
    let top_margin = 2.0f64.mul_add(OP_INSET, op_f);

    // Collect slots in polyomino order (canonical for Tab traversal).
    let slots: SlotList = puzzle
        .slots()
        .map(|slot| {
            let cells = slot
                .polyomino()
                .cells()
                .map(|c| (c.row, c.column))
                .collect();
            let cage = slot.as_cage().cloned();
            (cells, cage)
        })
        .collect();

    let slot_cells: Vec<Vec<(usize, usize)>> = slots.iter().map(|(c, _)| c.clone()).collect();
    let slot_cages: Vec<Option<mathdoku::Cage>> =
        slots.iter().map(|(_, cage)| cage.clone()).collect();
    let (colors, cell_slot) = assign_colors(n, &slot_cells);

    // Per-cell domains.
    let mut domains = vec![vec![vec![]; n]; n];
    for (cell_ref, domain) in puzzle.domains() {
        domains[cell_ref.row][cell_ref.column] = domain.iter().collect::<Vec<u8>>();
    }

    let puzzle_ref = PuzzleRef::new(puzzle, slot_cages);

    let grid_size = cell * n as f64;
    let total = 2.0f64.mul_add(MARGIN, grid_size);
    let vb = format!("0 0 {total} {total}");

    // ---- Interaction state ----

    let mode = RwSignal::new(initial_view.mode);
    let selected_cell = RwSignal::new((initial_view.cell_row, initial_view.cell_col));
    let selected_slot = RwSignal::new(initial_view.slot_idx);
    let provisional_region: RwSignal<Vec<(usize, usize)>> = RwSignal::new(Vec::new());
    let undo_stack: RwSignal<Vec<UndoEntry>> = RwSignal::new(Vec::new());

    provide_context(GridContext {
        mode,
        selected_cell,
        selected_slot,
        slot_cells: slot_cells.clone(),
        cell_slot: cell_slot.clone(),
        puzzle_ref,
        cell,
        provisional_region,
        undo_stack,
    });

    // Persist view state whenever mode/cell/slot changes.
    Effect::new(move |_| {
        #[derive(serde::Serialize)]
        struct Args {
            view: ViewState,
        }
        let view = ViewState {
            mode: mode.get(),
            cell_row: selected_cell.get().0,
            cell_col: selected_cell.get().1,
            slot_idx: selected_slot.get(),
        };
        leptos::task::spawn_local(async move {
            if let Ok(args) = serde_wasm_bindgen::to_value(&Args { view }) {
                invoke("set_view_state", args).await;
            }
        });
    });

    let slot_cells_static = slot_cells;
    let num_slots = slots.len();
    let cell_slot_kd = cell_slot.clone();

    // Returns true if (r, c) is edge-connected to any cell in `region`.
    let is_adjacent = |region: &[(usize, usize)], r: usize, c: usize| {
        region
            .iter()
            .any(|&(pr, pc)| (pr == r && pc.abs_diff(c) == 1) || (pc == c && pr.abs_diff(r) == 1))
    };

    let on_keydown = move |ev: leptos::ev::KeyboardEvent| {
        let key = ev.key();
        let shift = ev.shift_key();
        match mode.get_untracked() {
            Mode::Cell => {
                let (r, c) = selected_cell.get_untracked();

                // Shift+Arrow: provisional region drawing.
                if shift
                    && matches!(
                        key.as_str(),
                        "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight"
                    )
                {
                    ev.prevent_default();
                    // Current cell must be uncovered.
                    if cell_slot_kd[r][c].is_some() {
                        return;
                    }
                    // Compute target cell.
                    let target = match key.as_str() {
                        "ArrowUp" if r > 0 => Some((r - 1, c)),
                        "ArrowDown" if r + 1 < n => Some((r + 1, c)),
                        "ArrowLeft" if c > 0 => Some((r, c - 1)),
                        "ArrowRight" if c + 1 < n => Some((r, c + 1)),
                        _ => None,
                    };
                    let Some((tr, tc)) = target else { return };
                    // Target must be uncovered.
                    if cell_slot_kd[tr][tc].is_some() {
                        return;
                    }
                    let mut region = provisional_region.get_untracked();
                    if region.is_empty() {
                        // Start new provisional region from current cell.
                        region.push((r, c));
                        undo_stack.update(|s| s.push(UndoEntry::AddProvisionalCell));
                    } else if !is_adjacent(&region, r, c) {
                        // Current cell not connected to existing region: restart.
                        region.clear();
                        undo_stack.update(|s| {
                            while matches!(s.last(), Some(UndoEntry::AddProvisionalCell)) {
                                s.pop();
                            }
                        });
                        region.push((r, c));
                        undo_stack.update(|s| s.push(UndoEntry::AddProvisionalCell));
                    } else if !region.contains(&(r, c)) {
                        // Current cell is adjacent but not yet in region.
                        region.push((r, c));
                        undo_stack.update(|s| s.push(UndoEntry::AddProvisionalCell));
                    }
                    // Always add the target cell.
                    if !region.contains(&(tr, tc)) {
                        region.push((tr, tc));
                        undo_stack.update(|s| s.push(UndoEntry::AddProvisionalCell));
                    }
                    provisional_region.set(region);
                    selected_cell.set((tr, tc));
                    return;
                }

                match key.as_str() {
                    "ArrowUp" => {
                        ev.prevent_default();
                        if r > 0 {
                            selected_cell.set((r - 1, c));
                        }
                    }
                    "ArrowDown" => {
                        ev.prevent_default();
                        if r + 1 < n {
                            selected_cell.set((r + 1, c));
                        }
                    }
                    "ArrowLeft" => {
                        ev.prevent_default();
                        if c > 0 {
                            selected_cell.set((r, c - 1));
                        }
                    }
                    "ArrowRight" => {
                        ev.prevent_default();
                        if c + 1 < n {
                            selected_cell.set((r, c + 1));
                        }
                    }
                    "Tab" => {
                        ev.prevent_default();
                        if num_slots > 0 {
                            let slot_idx = slot_cells_static
                                .iter()
                                .position(|cells| cells.contains(&(r, c)))
                                .unwrap_or(0);
                            selected_slot.set(slot_idx);
                            mode.set(Mode::Slot);
                        }
                    }
                    "Escape" => {
                        ev.prevent_default();
                        let region = provisional_region.get_untracked();
                        if !region.is_empty() {
                            provisional_region.set(Vec::new());
                            undo_stack.update(|s| {
                                while matches!(s.last(), Some(UndoEntry::AddProvisionalCell)) {
                                    s.pop();
                                }
                            });
                        }
                    }
                    "Enter" => {
                        ev.prevent_default();
                        let region = provisional_region.get_untracked();
                        // Cells to commit: provisional region, or singleton current cell.
                        let commit_cells = if region.is_empty() {
                            if cell_slot_kd[r][c].is_some() {
                                return; // covered cell, nothing to do
                            }
                            vec![(r, c)]
                        } else {
                            region.clone()
                        };
                        #[derive(serde::Serialize)]
                        struct CellArg {
                            row: usize,
                            column: usize,
                        }
                        #[derive(serde::Serialize)]
                        struct AddRegionArgs {
                            cells: Vec<CellArg>,
                        }
                        let cells_arg: Vec<CellArg> = commit_cells
                            .iter()
                            .map(|&(row, column)| CellArg { row, column })
                            .collect();
                        let commit_cells_clone = commit_cells.clone();
                        leptos::task::spawn_local(async move {
                            let args =
                                serde_wasm_bindgen::to_value(&AddRegionArgs { cells: cells_arg });
                            let Ok(args) = args else { return };
                            let result = invoke("add_region", args).await;
                            let Ok(new_puzzle) =
                                serde_wasm_bindgen::from_value::<KenkenPuzzle>(result)
                            else {
                                return;
                            };
                            // Find the slot index of the new region in the new puzzle.
                            let new_slot_idx = new_puzzle
                                .slots()
                                .position(|slot| {
                                    let cells: std::collections::HashSet<_> = slot
                                        .polyomino()
                                        .cells()
                                        .map(|c| (c.row, c.column))
                                        .collect();
                                    commit_cells_clone.iter().all(|cell| cells.contains(cell))
                                        && cells.len() == commit_cells_clone.len()
                                })
                                .unwrap_or(0);
                            provisional_region.set(Vec::new());
                            undo_stack.update(|s| {
                                // Replace provisional cell entries with a single CommitRegion entry.
                                while matches!(s.last(), Some(UndoEntry::AddProvisionalCell)) {
                                    s.pop();
                                }
                                s.push(UndoEntry::CommitRegion {
                                    cells: commit_cells_clone,
                                });
                            });
                            let new_view = ViewState {
                                mode: Mode::Slot,
                                cell_row: 0,
                                cell_col: 0,
                                slot_idx: new_slot_idx,
                            };
                            on_puzzle_change.run((new_puzzle, new_view));
                        });
                    }
                    _ => {}
                }
            }
            Mode::Slot => {
                let idx = selected_slot.get_untracked();
                match key.as_str() {
                    "Tab" if !ev.shift_key() => {
                        ev.prevent_default();
                        selected_slot.set((idx + 1) % num_slots.max(1));
                    }
                    "Tab" => {
                        ev.prevent_default();
                        selected_slot.set(if idx == 0 {
                            num_slots.saturating_sub(1)
                        } else {
                            idx - 1
                        });
                    }
                    "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" => {
                        ev.prevent_default();
                        let anchor_cell = slot_cells_static
                            .get(idx)
                            .map_or((0, 0), |cells| anchor(cells));
                        selected_cell.set(anchor_cell);
                        mode.set(Mode::Cell);
                        let (r, c) = anchor_cell;
                        match key.as_str() {
                            "ArrowUp" if r > 0 => selected_cell.set((r - 1, c)),
                            "ArrowDown" if r + 1 < n => selected_cell.set((r + 1, c)),
                            "ArrowLeft" if c > 0 => selected_cell.set((r, c - 1)),
                            "ArrowRight" if c + 1 < n => selected_cell.set((r, c + 1)),
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    };

    // ---- Build static elements ----

    let cells_view: Vec<_> = (0..n)
        .flat_map(|r| (0..n).map(move |c| (r, c)))
        .map(|(r, c)| {
            let (x, y) = origin(cell, r, c);
            let fill = cell_slot[r][c].map_or(BG, |i| CAGE_PALETTE[colors[i] % CAGE_PALETTE.len()]);
            let domain = domains[r][c].clone();
            view! { <Cell x=x y=y cell=cell domain=domain fill=fill top_margin=top_margin n=n /> }
        })
        .collect();

    let slots_view: Vec<_> = slots
        .iter()
        .map(|(cells, cage)| {
            let (ar, ac) = anchor(cells);
            let (x, y) = origin(cell, ar, ac);
            cage.as_ref().map_or_else(
                || view! { <Region x=x y=y op_f=op_f /> }.into_any(),
                |c| {
                    let operation = c.operation();
                    view! { <Cage x=x y=y op_f=op_f operation=operation /> }.into_any()
                },
            )
        })
        .collect();

    // Gridlines.
    let mut lines = Vec::new();
    #[allow(clippy::needless_range_loop)]
    for r in 0..n.saturating_sub(1) {
        for c in 0..n {
            let thick = is_thick(cell_slot[r][c], cell_slot[r + 1][c]);
            let (stroke, width) = if thick { (INK, THICK) } else { (LINE, THIN) };
            let x1 = origin(cell, 0, c).0;
            let x2 = x1 + cell;
            let y = origin(cell, r + 1, 0).1;
            lines.push(view! {
                <line x1=x1 y1=y x2=x2 y2=y stroke=stroke stroke-width=width stroke-linecap="round" />
            });
        }
    }
    #[allow(clippy::needless_range_loop)]
    for c in 0..n.saturating_sub(1) {
        for r in 0..n {
            let thick = is_thick(cell_slot[r][c], cell_slot[r][c + 1]);
            let (stroke, width) = if thick { (INK, THICK) } else { (LINE, THIN) };
            let x = origin(cell, 0, c + 1).0;
            let y1 = origin(cell, r, 0).1;
            let y2 = y1 + cell;
            lines.push(view! {
                <line x1=x y1=y1 x2=x y2=y2 stroke=stroke stroke-width=width stroke-linecap="round" />
            });
        }
    }

    // Autofocus on mount.
    Effect::new(move |_| {
        use wasm_bindgen::JsCast;
        if let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.query_selector(".grid-svg").ok().flatten())
            .and_then(|el| el.dyn_into::<web_sys::SvgElement>().ok())
        {
            let _ = el.focus();
        }
    });

    view! {
        <div class="puzzle-wrap">
            <svg
                class="grid-svg"
                viewBox=vb
                xmlns="http://www.w3.org/2000/svg"
                tabindex="0"
                on:keydown=on_keydown
                style="outline:none;"
            >
                <rect x="0" y="0" width=total height=total fill=BG />
                {cells_view}
                {slots_view}
                {lines}
                <rect
                    x=MARGIN y=MARGIN
                    width=grid_size height=grid_size
                    fill="none"
                    stroke=INK
                    stroke-width=THICK
                />
                <SelectionOverlay />
            </svg>
            <div class="puzzle-footer">
                <CageStats />
                <SolutionCount />
            </div>
        </div>
    }
}

// ---- tests ----

#[cfg(test)]
mod tests {
    use super::*;
    use mathdoku::Operation;

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
    fn op_label_add() {
        assert_eq!(super::super::cage::op_label(Operation::Add(5)), "+5");
    }

    #[test]
    fn op_label_subtract() {
        assert_eq!(
            super::super::cage::op_label(Operation::Subtract(2)),
            "\u{2212}2"
        );
    }

    #[test]
    fn op_label_multiply() {
        assert_eq!(
            super::super::cage::op_label(Operation::Multiply(12)),
            "\u{00d7}12"
        );
    }

    #[test]
    fn op_label_divide() {
        assert_eq!(
            super::super::cage::op_label(Operation::Divide(3)),
            "\u{00f7}3"
        );
    }

    #[test]
    fn op_label_given_shows_only_target() {
        assert_eq!(super::super::cage::op_label(Operation::Given(7)), "7");
    }

    #[test]
    fn neighbors_center_has_four() {
        assert_eq!(neighbors(2, 2, 5).len(), 4);
    }

    #[test]
    fn neighbors_corner_has_two() {
        assert_eq!(neighbors(0, 0, 4).len(), 2);
    }

    #[test]
    fn neighbors_edge_has_three() {
        assert_eq!(neighbors(0, 2, 5).len(), 3);
    }

    #[test]
    fn neighbors_1x1_grid_is_empty() {
        assert!(neighbors(0, 0, 1).is_empty());
    }

    #[test]
    fn anchor_single_cell() {
        assert_eq!(anchor(&[(3, 2)]), (3, 2));
    }

    #[test]
    fn anchor_picks_leftmost_then_topmost() {
        assert_eq!(anchor(&[(1, 0), (0, 1)]), (1, 0));
    }

    #[test]
    fn anchor_tiebreaks_by_row() {
        assert_eq!(anchor(&[(2, 1), (0, 1)]), (0, 1));
    }

    #[test]
    fn anchor_empty_returns_default() {
        assert_eq!(anchor(&[]), (0, 0));
    }

    #[test]
    fn is_thick_both_same_slot_is_thin() {
        assert!(!is_thick(Some(0), Some(0)));
    }

    #[test]
    fn is_thick_different_slots_is_thick() {
        assert!(is_thick(Some(0), Some(1)));
    }

    #[test]
    fn is_thick_one_none_is_thick() {
        assert!(is_thick(Some(0), None));
        assert!(is_thick(None, Some(0)));
    }

    #[test]
    fn is_thick_both_none_is_thick() {
        assert!(is_thick(None, None));
    }

    #[test]
    fn assign_colors_empty_slots() {
        let (colors, _) = assign_colors(4, &[]);
        assert_eq!(colors, Vec::<usize>::new());
    }

    #[test]
    fn assign_colors_single_slot() {
        let slots = vec![vec![(0, 0), (0, 1)]];
        let (colors, _) = assign_colors(4, &slots);
        assert_eq!(colors.len(), 1);
    }

    #[test]
    fn assign_colors_adjacent_slots_get_different_colors() {
        let slots = vec![vec![(0, 0)], vec![(0, 1)]];
        let (colors, _) = assign_colors(4, &slots);
        assert_ne!(colors[0], colors[1]);
    }

    #[test]
    fn assign_colors_non_adjacent_slots_differ_from_their_neighbors() {
        let slots = vec![vec![(0, 0)], vec![(0, 1)], vec![(0, 2)]];
        let (colors, _) = assign_colors(4, &slots);
        assert_ne!(colors[0], colors[1]);
        assert_ne!(colors[1], colors[2]);
    }

    #[test]
    fn assign_colors_four_adjacent_slots_get_distinct_colors() {
        let slots: Vec<Vec<(usize, usize)>> = (0..4).map(|c| vec![(0, c)]).collect();
        let (colors, _) = assign_colors(4, &slots);
        assert_ne!(colors[0], colors[1]);
        assert_ne!(colors[1], colors[2]);
        assert_ne!(colors[2], colors[3]);
    }

    #[test]
    fn mode_cell_and_slot_are_distinct() {
        assert_ne!(Mode::Cell, Mode::Slot);
    }

    #[test]
    fn mode_copy_is_independent() {
        let a = Mode::Cell;
        let b = a;
        assert_eq!(a, b);
    }
}
