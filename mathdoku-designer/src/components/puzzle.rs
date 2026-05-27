//! Puzzle component: SVG root, layout, interaction, and subcomponent orchestration.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::items_after_statements, // `use wasm_bindgen::JsCast` inside the focus Effect
    clippy::needless_range_loop,    // 2D index loops are clearer with explicit row/col indices
    unused_results,                 // Effect::new/HashSet::insert/Vec::pop are fire-and-forget in reactive WASM code
)]

use leptos::prelude::*;
use leptos::task::spawn_local;
use mathdoku::{Cage, Cell, Grid, M, Operator, Polyomino, operators};
use mathdoku_designer_shared::State;

use super::cage::Cage as CageComponent;
use super::cage_stats::CageStats;
use super::cell::Cell as CellComponent;
use super::operation_selector::{FeasibilityState, OperationSelector, PendingCommit, handle_key};
use super::selection::{ProvisionalFills, SelectionOverlay};
use super::solution_count::SolutionCount;
use crate::cage_commit::{commit_cage, delete_cage, demote_cage};
use crate::geometry::{
    MARGIN, THICK, THIN, anchor, assign_colors, cell_size, is_thick, op_font, origin,
};
use crate::ipc;
use crate::partial_solution::PartialSolution;

use crate::keys::{
    ARROW_DOWN, ARROW_LEFT, ARROW_RIGHT, ARROW_UP, BACKSPACE, DELETE, ENTER, ESCAPE, TAB,
};
use crate::theme::{BG, CAGE_PALETTE, INK, LINE, OP_INSET};

#[component]
#[allow(clippy::needless_pass_by_value, clippy::too_many_lines)]
pub fn Puzzle(
    state: State,
    undo_stack: RwSignal<Vec<State>>,
    redo_stack: RwSignal<Vec<State>>,
    pending_commit: RwSignal<Option<PendingCommit>>,
    on_puzzle_change: Callback<State>,
    on_state_change: Callback<State>,
    on_error: Callback<String>,
) -> impl IntoView {
    let n = state.puzzle.n();
    let cell = cell_size(n);
    let op_f = op_font(cell);
    let top_margin = 2.0f64.mul_add(OP_INSET, op_f);

    // Collect cages in polyomino order (canonical for Tab traversal).
    let cages: CageList = state
        .puzzle
        .cages()
        .map(|cage| (cage.cells(), cage.clone()))
        .collect();

    let cage_cells: Vec<Vec<Cell>> = cages.iter().map(|(c, _)| c.clone()).collect();
    let (colors, cage_index) = assign_colors(n, &cage_cells);

    // Propagate cage constraints from an unconstrained grid so each cell's
    // domain shows all values still possible given the cages, not just the solution.
    let propagated = Grid::new(n)
        .and_then(|g| g.constrain(&state.puzzle))
        .unwrap_or_else(|_| state.current.clone());
    let mut domains = vec![vec![vec![]; n]; n];
    let mut solution_values = vec![vec![None::<u8>; n]; n];
    for (r, row) in domains.iter_mut().enumerate() {
        for (c, cell_domain) in row.iter_mut().enumerate() {
            let cell_ref = Cell::new(r, c);
            if let Ok(vals) = propagated.cell_values(cell_ref) {
                *cell_domain = vals.values();
            }
            // Without-Solution mode has no solution values to overlay.
            if let Some(solution) = &state.solution
                && let Ok(sv) = solution.cell_values(cell_ref)
            {
                solution_values[r][c] = sv.values().first().copied();
            }
        }
    }

    let partial_solution = PartialSolution::new(state.puzzle.clone(), state.current.clone());

    let grid_size = cell * n as f64;
    let total = 2.0f64.mul_add(MARGIN, grid_size);
    let vb = format!("0 0 {total} {total}");

    // ---- Interaction state ----

    // `designer_state` is the single source of truth for active cell and provisional cages.
    let designer_state: RwSignal<State> = RwSignal::new(state);

    let partial_solution_kd = partial_solution.clone();
    provide_context(InteractionState {
        designer_state,
        partial_solution,
        cell_size: cell,
        pending_commit,
    });

    // Persist the active cell whenever it changes.
    Effect::new(move |_| {
        let active = designer_state.get().active;
        spawn_local(async move {
            let _ = ipc::set_active_cell(active).await;
        });
    });

    let partial_solution = partial_solution_kd;
    let cage_cells_static = cage_cells;
    let num_cages = cages.len();

    // `Fix Solution` is only valid when the puzzle currently has exactly one
    // completion (the backend rejects `fix` otherwise). Compute that once — the
    // component re-mounts on every puzzle change — and only in Without-Solution
    // mode, where the Fix button is shown. `None` while the solver runs keeps
    // the button disabled.
    let has_unique_solution: RwSignal<Option<bool>> = RwSignal::new(None);
    if designer_state.get_untracked().solution.is_none() {
        let ps = partial_solution.clone();
        spawn_local(async move {
            has_unique_solution.set(Some(ps.solution_count() == Some(1)));
        });
    }

    // Helper: apply a lightweight navigation state change (no undo entry).
    let set_state = move |new_st: State| {
        on_state_change.run(new_st.clone());
        designer_state.set(new_st);
    };

    // Helper: open the operation selector for a given polyomino.
    // Used by Enter (provisional → selector) and Escape (committed cage → demote → selector).
    // Singletons (Given only) are skipped — they stay as provisional cages without a selector.
    let open_selector = Callback::new(move |poly: Polyomino| {
        let st = designer_state.get_untracked();
        let without_solution = st.solution.is_none();
        let allowed = operators(&poly);
        // With-Solution singletons commit immediately (Given target read from the
        // solution); they never open the selector. Without-Solution singletons
        // need a target chosen, so they do open it.
        if !without_solution && allowed == [Operator::Given] {
            return;
        }
        let poly_for_cb = poly.clone();
        let parked: std::collections::BTreeSet<Polyomino> = st
            .provisional_cages
            .iter()
            .filter(|p| p.cells() != poly.cells())
            .cloned()
            .collect();
        let on_commit = Callback::new(move |(op, target): (Operator, Option<M>)| {
            pending_commit.set(None);
            commit_cage(
                &poly_for_cb,
                op,
                target,
                parked.clone(),
                undo_stack,
                redo_stack,
                designer_state,
                on_puzzle_change,
                on_error,
            );
        });
        let selected_idx = RwSignal::new(0usize);
        let picked_operator = RwSignal::new(None);
        // Without-Solution mode computes the globally-feasible (op, target) pairs
        // for the dropdown, showing a spinner via the Computing state meanwhile.
        let feasible = without_solution.then(|| {
            let sig = RwSignal::new(FeasibilityState::Computing);
            let puzzle = st.puzzle.clone();
            let poly_for_query = poly.clone();
            spawn_local(async move {
                let pairs =
                    crate::feasibility::cached_feasible_op_targets(&puzzle, &poly_for_query);
                sig.set(FeasibilityState::Ready(pairs));
            });
            sig
        });
        pending_commit.set(Some(PendingCommit {
            polyomino: poly,
            allowed,
            selected_idx,
            on_commit,
            feasible,
            picked_operator,
        }));
    });

    // Helper: swap the undo/redo stacks and apply the restored state.
    let apply_history = move |from: RwSignal<Vec<State>>, to: RwSignal<Vec<State>>| {
        if let Some(restored) = from.get_untracked().last().cloned() {
            from.update(|s| {
                s.pop();
            });
            to.update(|s| s.push(designer_state.get_untracked()));
            on_state_change.run(restored.clone());
            on_puzzle_change.run(restored.clone());
            designer_state.set(restored);
        }
    };

    // Mode switching: `fix` snapshots the unique completion, `unfix` drops it.
    // Both go through the backend (which owns the persisted solution) and are
    // pushed onto the undo stack like any other puzzle change.
    let on_fix = Callback::new(move |(): ()| {
        spawn_local(async move {
            match ipc::fix().await {
                Ok(mut new_st) => {
                    let pre = designer_state.get_untracked();
                    new_st.provisional_cages.clone_from(&pre.provisional_cages);
                    new_st.active = pre.active;
                    undo_stack.update(|s| s.push(pre));
                    redo_stack.update(std::vec::Vec::clear);
                    designer_state.set(new_st.clone());
                    on_puzzle_change.run(new_st);
                }
                Err(e) => on_error.run(e.to_string()),
            }
        });
    });
    let on_unfix = Callback::new(move |(): ()| {
        spawn_local(async move {
            match ipc::unfix().await {
                Ok(mut new_st) => {
                    let pre = designer_state.get_untracked();
                    new_st.provisional_cages.clone_from(&pre.provisional_cages);
                    new_st.active = pre.active;
                    undo_stack.update(|s| s.push(pre));
                    redo_stack.update(std::vec::Vec::clear);
                    designer_state.set(new_st.clone());
                    on_puzzle_change.run(new_st);
                }
                Err(e) => on_error.run(e.to_string()),
            }
        });
    });

    let on_keydown = move |ev: leptos::ev::KeyboardEvent| {
        let key = ev.key();
        let shift = ev.shift_key();
        let st = designer_state.get_untracked();
        let (r, c) = (st.active.row, st.active.column);

        // Operation selector intercepts all keys when active.
        if let Some(p) = pending_commit.get_untracked() {
            ev.prevent_default();
            handle_key(
                key.as_str(),
                shift,
                &p,
                pending_commit,
                designer_state,
                on_state_change,
            );
            return;
        }

        // Cmd+Z: undo. Cmd+Shift+Z: redo.
        if ev.meta_key() && key.as_str() == "z" {
            ev.prevent_default();
            if shift {
                apply_history(redo_stack, undo_stack);
            } else {
                apply_history(undo_stack, redo_stack);
            }
            return;
        }

        // Shift+Arrow: provisional cage drawing.
        if shift
            && matches!(
                key.as_str(),
                ARROW_UP | ARROW_DOWN | ARROW_LEFT | ARROW_RIGHT
            )
        {
            ev.prevent_default();
            // Current cell must be uncovered.
            if partial_solution.cage_index_at(r, c).is_some() {
                return;
            }
            // Compute target cell.
            let target = match key.as_str() {
                ARROW_UP if r > 0 => Some((r - 1, c)),
                ARROW_DOWN if r + 1 < n => Some((r + 1, c)),
                ARROW_LEFT if c > 0 => Some((r, c - 1)),
                ARROW_RIGHT if c + 1 < n => Some((r, c + 1)),
                _ => None,
            };
            let Some((tr, tc)) = target else { return };
            // Target must be uncovered.
            if partial_solution.cage_index_at(tr, tc).is_some() {
                return;
            }
            set_state(step_provisional_cage(r, c, tr, tc, st));
            return;
        }

        match key.as_str() {
            ARROW_UP if r > 0 => {
                ev.prevent_default();
                set_state(State {
                    active: Cell::new(r - 1, c),
                    ..st
                });
            }
            ARROW_DOWN if r + 1 < n => {
                ev.prevent_default();
                set_state(State {
                    active: Cell::new(r + 1, c),
                    ..st
                });
            }
            ARROW_LEFT if c > 0 => {
                ev.prevent_default();
                set_state(State {
                    active: Cell::new(r, c - 1),
                    ..st
                });
            }
            ARROW_RIGHT if c + 1 < n => {
                ev.prevent_default();
                set_state(State {
                    active: Cell::new(r, c + 1),
                    ..st
                });
            }
            ARROW_UP | ARROW_DOWN | ARROW_LEFT | ARROW_RIGHT => {
                ev.prevent_default(); // at boundary — consume but don't move
            }
            TAB => {
                ev.prevent_default();
                if num_cages > 0 {
                    let here = Cell::new(r, c);
                    let current_cage = cage_cells_static
                        .iter()
                        .position(|cells| cells.contains(&here))
                        .unwrap_or(0);
                    let next_cage = if shift {
                        if current_cage == 0 {
                            num_cages - 1
                        } else {
                            current_cage - 1
                        }
                    } else {
                        (current_cage + 1) % num_cages
                    };
                    set_state(State {
                        active: anchor(&cage_cells_static[next_cage]),
                        ..st
                    });
                }
            }
            ESCAPE => {
                ev.prevent_default();
                let active_cell = Cell::new(r, c);
                if let Some(cage_idx) = partial_solution.cage_index_at(r, c) {
                    // Active cell is in a committed cage — demote it to provisional.
                    let cells = cage_cells_static[cage_idx].clone();
                    demote_cage(
                        cells,
                        undo_stack,
                        redo_stack,
                        designer_state,
                        on_puzzle_change,
                        open_selector,
                        on_error,
                    );
                } else if let Some(poly) = st
                    .provisional_cages
                    .iter()
                    .find(|p| p.cells().contains(&active_cell))
                    .cloned()
                {
                    // Active cell is in a provisional cage — delete it.
                    let mut new_st = st.clone();
                    new_st.provisional_cages.remove(&poly);
                    set_state(new_st);
                } // else: uncovered cell — do nothing
            }
            DELETE | BACKSPACE => {
                ev.prevent_default();
                let active_cell = Cell::new(r, c);
                if let Some(cage_idx) = partial_solution.cage_index_at(r, c) {
                    // Active cell is in a committed cage — delete it outright
                    // (no demotion to a provisional cage).
                    let cells = cage_cells_static[cage_idx].clone();
                    delete_cage(
                        cells,
                        undo_stack,
                        redo_stack,
                        designer_state,
                        on_puzzle_change,
                        on_error,
                    );
                } else if let Some(poly) = st
                    .provisional_cages
                    .iter()
                    .find(|p| p.cells().contains(&active_cell))
                    .cloned()
                {
                    // Active cell is in a provisional cage — delete it.
                    let mut new_st = st.clone();
                    new_st.provisional_cages.remove(&poly);
                    set_state(new_st);
                } // else: uncovered cell — do nothing
            }
            ENTER => {
                ev.prevent_default();
                let active_cell = Cell::new(r, c);
                // Cells to commit: active provisional cage, or a fresh singleton.
                let poly = if let Some(p) = st
                    .provisional_cages
                    .iter()
                    .find(|p| p.cells().contains(&active_cell))
                    .cloned()
                {
                    p
                } else {
                    if partial_solution.cage_index_at(r, c).is_some() {
                        return; // covered cell, nothing to do
                    }
                    let Ok(p) = Polyomino::from_cells(&[active_cell]) else {
                        return; // should never happen for a single cell
                    };
                    p
                };
                // With-Solution singleton: Given with a solution-derived target —
                // commit immediately. Without-Solution singletons need a target
                // chosen, so they fall through to the operation selector.
                if st.solution.is_some() && operators(&poly) == [Operator::Given] {
                    let parked: std::collections::BTreeSet<Polyomino> = st
                        .provisional_cages
                        .iter()
                        .filter(|p| p.cells() != poly.cells())
                        .cloned()
                        .collect();
                    commit_cage(
                        &poly,
                        Operator::Given,
                        None,
                        parked,
                        undo_stack,
                        redo_stack,
                        designer_state,
                        on_puzzle_change,
                        on_error,
                    );
                    return;
                }
                // Multi-cell, or a Without-Solution singleton: show the operation selector.
                open_selector.run(poly);
            }
            _ => {}
        }
    };

    // ---- Build static elements ----

    let cells_view: Vec<_> = (0..n)
        .flat_map(|r| (0..n).map(move |c| (r, c)))
        .map(|(r, c)| {
            let (x, y) = origin(cell, r, c);
            let fill = cage_index[r][c].map_or(BG, |i| CAGE_PALETTE[colors[i] % CAGE_PALETTE.len()]);
            let domain = domains[r][c].clone();
            let solution_value = solution_values[r][c];
            view! { <CellComponent x=x y=y cell=cell domain=domain fill=fill top_margin=top_margin n=n solution_value=solution_value /> }
        })
        .collect();

    let cages_view: Vec<_> = cages
        .iter()
        .map(|(cells, cage)| {
            let a = anchor(cells);
            let (x, y) = origin(cell, a.row, a.column);
            let operation = cage.operation();
            view! { <CageComponent x=x y=y op_f=op_f operation=operation /> }.into_any()
        })
        .collect();

    // Gridlines.
    let mut lines = Vec::new();
    for r in 0..n.saturating_sub(1) {
        for c in 0..n {
            let thick = is_thick(cage_index[r][c], cage_index[r + 1][c]);
            let (stroke, width) = if thick { (INK, THICK) } else { (LINE, THIN) };
            let x1 = origin(cell, 0, c).0;
            let x2 = x1 + cell;
            let y = origin(cell, r + 1, 0).1;
            lines.push(view! {
                <line x1=x1 y1=y x2=x2 y2=y stroke=stroke stroke-width=width stroke-linecap="round" />
            });
        }
    }
    for c in 0..n.saturating_sub(1) {
        for r in 0..n {
            let thick = is_thick(cage_index[r][c], cage_index[r][c + 1]);
            let (stroke, width) = if thick { (INK, THICK) } else { (LINE, THIN) };
            let x = origin(cell, 0, c + 1).0;
            let y1 = origin(cell, r, 0).1;
            let y2 = y1 + cell;
            lines.push(view! {
                <line x1=x y1=y1 x2=x y2=y2 stroke=stroke stroke-width=width stroke-linecap="round" />
            });
        }
    }

    // Focus the SVG on mount and whenever the operation selector opens/closes.
    Effect::new(move |_| {
        let _ = pending_commit.get(); // re-run when selector changes
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
                {cages_view}
                <ProvisionalFills />
                {lines}
                <rect
                    x=MARGIN y=MARGIN
                    width=grid_size height=grid_size
                    fill="none"
                    stroke=INK
                    stroke-width=THICK
                />
                <SelectionOverlay />
                <OperationSelector />
            </svg>
            <div class="puzzle-footer">
                <CageStats />
                {move || {
                    // `Fix Solution` is offered in Without-Solution mode; the backend
                    // rejects it unless the puzzle has exactly one completion, so it is
                    // disabled until a unique solution exists. `Unfix Solution` is
                    // offered in With-Solution mode. The button keeps a fixed width (set
                    // in CSS) so its size never changes between the two labels.
                    let btn_style = format!(
                        "padding:4px 10px;border:0.5px solid {LINE};border-radius:5px;\
                         background:{BG};color:{INK};font-size:12px;cursor:pointer;"
                    );
                    if designer_state.get().solution.is_some() {
                        view! {
                            <button class="fix-solution-btn" style=btn_style on:click=move |_| on_unfix.run(())>"Unfix Solution"</button>
                        }.into_any()
                    } else {
                        let enabled = has_unique_solution.get() == Some(true);
                        let style = if enabled {
                            btn_style
                        } else {
                            format!("{btn_style}opacity:0.5;cursor:default;")
                        };
                        view! {
                            <button class="fix-solution-btn" style=style disabled=!enabled on:click=move |_| on_fix.run(())>"Fix Solution"</button>
                        }.into_any()
                    }
                }}
                <SolutionCount />
            </div>
        </div>
    }
}

// ---- context ----

/// Interaction state provided to all sub-components via context.
#[derive(Clone)]
pub struct InteractionState {
    /// Single source of truth: active cell and provisional cages.
    pub designer_state: RwSignal<State>,
    /// Cage structure and constrained domains for on-demand queries.
    pub partial_solution: PartialSolution,
    /// Cell size in SVG units.
    pub cell_size: f64,
    /// Cells awaiting operator selection before being committed as a cage.
    pub pending_commit: RwSignal<Option<PendingCommit>>,
}

/// Each entry is the cells of a cage (in library order) and the `Cage` itself.
type CageList = Vec<(Vec<Cell>, Cage)>;

// ---- Helpers ----

/// Advances the provisional cage one step during Shift+Arrow drawing.
///
/// Finds the provisional cage containing `(r, c)` (or starts a new singleton),
/// extends it to include `(tr, tc)`, and returns the updated `State`.
/// If `(r, c)` is disconnected from every existing provisional cage, the active
/// one is left as-is and a new singleton is started.
fn step_provisional_cage(r: usize, c: usize, tr: usize, tc: usize, state: State) -> State {
    use std::collections::BTreeSet;
    let current = Cell::new(r, c);
    let target = Cell::new(tr, tc);

    // Find the provisional cage that contains (or is adjacent to) current.
    let active = state
        .provisional_cages
        .iter()
        .find(|p| p.cells().contains(&current))
        .cloned();

    let (region, mut remaining): (Polyomino, BTreeSet<Polyomino>) = match active {
        None => {
            // No region contains current — start a new singleton.
            let Ok(new_region) = Polyomino::from_cells(&[current]) else {
                return state;
            };
            (new_region, state.provisional_cages.clone())
        }
        Some(poly) => {
            let rest: BTreeSet<Polyomino> = state
                .provisional_cages
                .iter()
                .filter(|p| *p != &poly)
                .cloned()
                .collect();
            if let Ok(extended) = poly.insert(current) {
                (extended, rest)
            } else {
                // Current cell disconnected from this cage — park it and start fresh.
                let mut parked = rest;
                parked.insert(poly);
                let Ok(new_region) = Polyomino::from_cells(&[current]) else {
                    return state;
                };
                (new_region, parked)
            }
        }
    };

    // Extend to include target (guaranteed adjacent — one step from current).
    let region = region.insert(target).unwrap_or(region);

    remaining.insert(region);
    State {
        active: Cell::new(tr, tc),
        provisional_cages: remaining,
        ..state
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::step_provisional_cage;
    use mathdoku::{Cell, Polyomino};
    use mathdoku_designer_shared::State;

    fn poly(positions: &[(usize, usize)]) -> Polyomino {
        let cells: Vec<Cell> = positions.iter().map(|&(r, c)| Cell::new(r, c)).collect();
        Polyomino::from_cells(&cells).unwrap()
    }

    fn cells_of(p: &Polyomino) -> Vec<(usize, usize)> {
        p.cells().into_iter().map(|c| (c.row, c.column)).collect()
    }

    #[test]
    fn starts_new_singleton_and_extends_to_target() {
        let state = State::new(4).unwrap();
        let result = step_provisional_cage(0, 0, 0, 1, state);

        assert_eq!(result.active, Cell::new(0, 1));
        assert_eq!(result.provisional_cages.len(), 1);
        let region = result.provisional_cages.iter().next().unwrap();
        assert_eq!(cells_of(region), vec![(0, 0), (0, 1)]);
    }

    #[test]
    fn extends_existing_provisional_cage() {
        let mut state = State::new(4).unwrap();
        assert!(state.provisional_cages.insert(poly(&[(0, 0), (0, 1)])));

        // Active cell (0,1) belongs to the existing cage; extend it to (0,2).
        let result = step_provisional_cage(0, 1, 0, 2, state);

        assert_eq!(result.active, Cell::new(0, 2));
        assert_eq!(result.provisional_cages.len(), 1);
        let region = result.provisional_cages.iter().next().unwrap();
        assert_eq!(cells_of(region), vec![(0, 0), (0, 1), (0, 2)]);
    }

    #[test]
    fn preserves_other_provisional_cages_when_starting_fresh() {
        let mut state = State::new(4).unwrap();
        assert!(state.provisional_cages.insert(poly(&[(3, 3), (3, 2)])));

        // (0,0) is not in any existing cage — a new region is started while the
        // unrelated cage is left untouched.
        let result = step_provisional_cage(0, 0, 1, 0, state);

        assert_eq!(result.active, Cell::new(1, 0));
        assert_eq!(result.provisional_cages.len(), 2);
        let has_new = result
            .provisional_cages
            .iter()
            .any(|p| cells_of(p) == vec![(0, 0), (1, 0)]);
        let has_old = result
            .provisional_cages
            .iter()
            .any(|p| cells_of(p) == vec![(3, 2), (3, 3)]);
        assert!(has_new, "new region should be present");
        assert!(has_old, "pre-existing region should be preserved");
    }

    #[test]
    fn extends_the_active_cage_leaving_others_alone() {
        let mut state = State::new(5).unwrap();
        assert!(state.provisional_cages.insert(poly(&[(0, 0), (0, 1)])));
        assert!(state.provisional_cages.insert(poly(&[(4, 4), (4, 3)])));

        let result = step_provisional_cage(0, 1, 0, 2, state);

        assert_eq!(result.active, Cell::new(0, 2));
        assert_eq!(result.provisional_cages.len(), 2);
        let has_extended = result
            .provisional_cages
            .iter()
            .any(|p| cells_of(p) == vec![(0, 0), (0, 1), (0, 2)]);
        let has_other = result
            .provisional_cages
            .iter()
            .any(|p| cells_of(p) == vec![(4, 3), (4, 4)]);
        assert!(has_extended);
        assert!(has_other);
    }
}
