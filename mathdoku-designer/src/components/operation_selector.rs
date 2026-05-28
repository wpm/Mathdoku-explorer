//! Operation selector: tab well shown in the anchor cell when the user is
//! choosing an operator (and, in Without-Solution mode, a target) for a pending
//! cage commit.
//!
//! Two rendering modes:
//!
//! - **With Solution** (`PendingCommit::feasible` is `None`): operator tabs whose
//!   labels carry the target computed from the fixed solution. Clicking a tab
//!   commits the cage; the backend derives the target.
//! - **Without Solution** (`feasible` is `Some`): a two-step picker. The operator
//!   strip lists the operators that admit a globally-feasible target; clicking
//!   one reveals its feasible targets. When no `(operator, target)` is feasible,
//!   an inline "no operation possible" message replaces the strip.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::suboptimal_flops, // layout arithmetic reads clearer as plain * and +
    unused_results            // Effect::new is fire-and-forget in reactive WASM code
)]

use leptos::prelude::*;
use mathdoku::{Operation, Operator, Polyomino, Target};

use super::puzzle::InteractionState;
use crate::feasibility::group_by_operator;
use crate::geometry::{anchor, origin};
use crate::partial_solution::PartialSolution;
use crate::theme::{ACCENT, BG, INK, INK2, LINE, SERIF};

/// Without-Solution dropdown computation state. The set of feasible
/// `(operator, target)` pairs is computed asynchronously so the picker can show
/// a spinner during a cache miss instead of blocking the UI.
#[derive(Clone)]
pub enum FeasibilityState {
    /// Feasibility is being computed; the picker shows a spinner.
    Computing,
    /// Computation finished with the given feasible pairs (possibly empty).
    Ready(Vec<(Operator, Target)>),
}

/// Floating tab well rendered over the anchor cell of the pending cage.
#[component]
pub fn OperationSelector() -> impl IntoView {
    #[allow(clippy::panic)]
    let ctx = use_context::<InteractionState>()
        .unwrap_or_else(|| panic!("OperationSelector must be inside Puzzle"));

    move || {
        let Some(pending) = ctx.pending_commit.get() else {
            return ().into_any();
        };

        let cell_size = ctx.cell_size;
        let a = anchor(&pending.polyomino.cells());
        let (x, y) = origin(cell_size, a.row, a.column);

        match pending.feasible {
            Some(feasible) => without_solution_view(&pending, feasible, cell_size, x, y),
            None => with_solution_view(&pending, &ctx.partial_solution, cell_size, x, y),
        }
    }
}

/// With-Solution rendering: operator tabs labelled with the solution-derived target.
fn with_solution_view(
    pending: &PendingCommit,
    partial_solution: &PartialSolution,
    cell_size: f64,
    x: f64,
    y: f64,
) -> AnyView {
    // Singleton: only Given is allowed — commit immediately without showing UI.
    // (The Enter handler already handles this case, but guard here too.)
    if pending.allowed == [Operator::Given] {
        pending.on_commit.run((Operator::Given, None));
        return ().into_any();
    }

    let tab_w = cell_size.clamp(44.0, 56.0);
    let tab_h = 28.0;
    let pad = 4.0;
    let gap = 2.0;
    let on_commit = pending.on_commit;
    let polyomino = pending.polyomino.clone();
    let selected_idx = pending.selected_idx;

    // Build (operator, label) pairs. When all cells' values are singletons,
    // omit any operator for which compute_target returns None (e.g. Divide
    // on non-divisible values). When values are undetermined, show all
    // allowed operators with a label of just the operator symbol.
    let all_determined = polyomino
        .cells()
        .iter()
        .all(|&c| partial_solution.cell_value_singleton(c).is_some());
    let ops: Vec<(Operator, String)> = pending
        .allowed
        .iter()
        .filter_map(|op| {
            let target = compute_target(&polyomino, op, partial_solution);
            if all_determined && target.is_none() {
                return None; // structurally invalid for these cell values
            }
            let label = target.map_or_else(
                || op.to_string(),
                |t| Operation::new(op.clone(), t).to_string(),
            );
            Some((op.clone(), label))
        })
        .collect();

    let n_tabs = ops.len();
    let total_w = tab_w * n_tabs as f64 + gap * (n_tabs - 1) as f64 + pad * 2.0;
    let total_h = tab_h + pad * 2.0;

    view! {
        <g>
            <rect
                x={x} y={y}
                width={total_w} height={total_h}
                rx="4"
                fill=BG
                stroke=LINE
                stroke-width="0.75"
            />
            {ops.into_iter().enumerate().map(|(i, (op, label))| {
                let tab_x = (tab_w + gap).mul_add(i as f64, x + pad);
                let tab_y = y + pad;
                let tx = tab_x + tab_w / 2.0;
                let ty = tab_y + tab_h / 2.0;
                view! {
                    <g
                        style="cursor:pointer;"
                        on:click=move |_| on_commit.run((op.clone(), None))
                    >
                        <rect
                            x={tab_x} y={tab_y}
                            width={tab_w} height={tab_h}
                            rx="3"
                            fill=move || if selected_idx.get() == i { ACCENT } else { BG }
                            stroke=ACCENT
                            stroke-width="1.0"
                        />
                        <text
                            x={tx} y={ty}
                            text-anchor="middle"
                            dominant-baseline="middle"
                            font-family=SERIF
                            font-size="14"
                            font-weight="700"
                            fill=move || if selected_idx.get() == i { BG } else { INK }
                        >{label}</text>
                    </g>
                }
            }).collect::<Vec<_>>()}
        </g>
    }
    .into_any()
}

/// Without-Solution rendering: spinner, empty-state message, operator strip, or
/// target sub-picker depending on the computation state and current selection.
fn without_solution_view(
    pending: &PendingCommit,
    feasible: RwSignal<FeasibilityState>,
    cell_size: f64,
    x: f64,
    y: f64,
) -> AnyView {
    let tab_w = cell_size.clamp(44.0, 56.0);
    let tab_h = 28.0;
    let pad = 4.0;
    let gap = 2.0;
    let on_commit = pending.on_commit;
    let picked = pending.picked_operator;
    let selected_idx = pending.selected_idx;

    match feasible.get() {
        FeasibilityState::Computing => spinner_view(x, y, tab_w, tab_h, pad),
        FeasibilityState::Ready(pairs) if pairs.is_empty() => empty_message_view(x, y),
        FeasibilityState::Ready(pairs) => picked.get().map_or_else(
            || operator_strip_view(&pairs, picked, selected_idx, tab_w, tab_h, pad, gap, x, y),
            |op| target_select_view(&pairs, &op, on_commit, tab_w, tab_h, pad, x, y),
        ),
    }
}

/// A small spinner shown in the anchor while feasibility is computing.
fn spinner_view(x: f64, y: f64, tab_w: f64, tab_h: f64, pad: f64) -> AnyView {
    let w = tab_w + pad * 2.0;
    let h = tab_h + pad * 2.0;
    view! {
        <g>
            <rect x={x} y={y} width={w} height={h} rx="4" fill=BG stroke=LINE stroke-width="0.75" />
            <text
                x={x + w / 2.0} y={y + h / 2.0}
                text-anchor="middle" dominant-baseline="middle"
                font-family=SERIF font-size="16" fill=INK2
            >"…"</text>
        </g>
    }
    .into_any()
}

/// The inline "no operation possible — redraw cage" message.
fn empty_message_view(x: f64, y: f64) -> AnyView {
    let w = 220.0;
    let h = 30.0;
    view! {
        <g>
            <rect x={x} y={y} width={w} height={h} rx="4" fill=BG stroke=LINE stroke-width="0.75" />
            <text
                x={x + 8.0} y={y + h / 2.0}
                text-anchor="start" dominant-baseline="middle"
                font-family=SERIF font-size="12" fill=INK2
            >"no operation possible \u{2014} redraw cage"</text>
        </g>
    }
    .into_any()
}

/// Step one: the operator strip. Clicking an operator opens its target picker.
#[allow(clippy::too_many_arguments)]
fn operator_strip_view(
    pairs: &[(Operator, Target)],
    picked: RwSignal<Option<Operator>>,
    selected_idx: RwSignal<usize>,
    tab_w: f64,
    tab_h: f64,
    pad: f64,
    gap: f64,
    x: f64,
    y: f64,
) -> AnyView {
    let ops: Vec<Operator> = group_by_operator(pairs)
        .into_iter()
        .map(|(op, _)| op)
        .collect();
    let n_tabs = ops.len();
    let total_w = tab_w * n_tabs as f64 + gap * (n_tabs - 1) as f64 + pad * 2.0;
    let total_h = tab_h + pad * 2.0;

    view! {
        <g>
            <rect
                x={x} y={y} width={total_w} height={total_h}
                rx="4" fill=BG stroke=LINE stroke-width="0.75"
            />
            {ops.into_iter().enumerate().map(|(i, op)| {
                let tab_x = (tab_w + gap).mul_add(i as f64, x + pad);
                let tab_y = y + pad;
                let tx = tab_x + tab_w / 2.0;
                let ty = tab_y + tab_h / 2.0;
                let label = op.to_string();
                view! {
                    <g
                        style="cursor:pointer;"
                        on:click=move |_| { selected_idx.set(0); picked.set(Some(op.clone())); }
                    >
                        <rect
                            x={tab_x} y={tab_y} width={tab_w} height={tab_h}
                            rx="3"
                            fill=move || if selected_idx.get() == i { ACCENT } else { BG }
                            stroke=ACCENT stroke-width="1.0"
                        />
                        <text
                            x={tx} y={ty}
                            text-anchor="middle" dominant-baseline="middle"
                            font-family=SERIF font-size="14" font-weight="700"
                            fill=move || if selected_idx.get() == i { BG } else { INK }
                        >{label}</text>
                    </g>
                }
            }).collect::<Vec<_>>()}
        </g>
    }
    .into_any()
}

/// Step two: a native `<select>` dropdown of the feasible targets for the chosen
/// operator, embedded via `<foreignObject>`. The browser renders the option list
/// in its top layer, so it never clips against the puzzle edge, and provides
/// focus, arrow-key, mouse, and type-to-select navigation for free. Options carry
/// the bare target number (so typing the number jumps to it); the operator symbol
/// is the placeholder. Choosing an option commits the cage with `(operator, target)`.
#[allow(clippy::too_many_arguments)]
fn target_select_view(
    pairs: &[(Operator, Target)],
    op: &Operator,
    on_commit: Callback<(Operator, Option<Target>)>,
    tab_w: f64,
    tab_h: f64,
    pad: f64,
    x: f64,
    y: f64,
) -> AnyView {
    let options: Vec<_> = pairs
        .iter()
        .filter(|(o, _)| o == op)
        .map(|(_, t)| {
            let value = t.to_string();
            let label = value.clone();
            view! { <option value=value>{label}</option> }
        })
        .collect();
    let fo_w = tab_w.max(56.0) + pad * 2.0;
    let fo_h = tab_h + pad * 2.0;
    // The operator symbol is the placeholder. `Given` renders with no symbol, so
    // a singleton's value dropdown shows a blank header rather than a `#`.
    let placeholder = op.to_string();
    let op_for_change = op.clone();

    // Move focus to the dropdown as soon as it mounts.
    let select_ref = NodeRef::<leptos::html::Select>::new();
    Effect::new(move |_| {
        if let Some(el) = select_ref.get() {
            let _ = el.focus();
        }
    });

    let select_style = format!(
        "width:100%;height:100%;box-sizing:border-box;\
         font-family:{SERIF};font-size:14px;font-weight:700;\
         color:{INK};background:{BG};border:1px solid {ACCENT};\
         border-radius:3px;padding:0 4px;cursor:pointer;"
    );

    view! {
        <foreignObject x={x} y={y} width={fo_w} height={fo_h}>
            <select
                node_ref=select_ref
                class="target-select"
                style=select_style
                on:change=move |ev| {
                    if let Ok(target) = event_target_value(&ev).parse::<Target>() {
                        on_commit.run((op_for_change.clone(), Some(target)));
                    }
                }
            >
                <option value="" disabled=true selected=true>{placeholder}</option>
                {options}
            </select>
        </foreignObject>
    }
    .into_any()
}

/// The polyomino pending a cage commit and a callback invoked with the chosen operator.
#[derive(Clone)]
pub struct PendingCommit {
    /// The cells that will form the new cage.
    pub polyomino: Polyomino,
    /// The operators that are valid for this cage's polyomino size (With-Solution
    /// operator tabs). Unused in Without-Solution mode, where the strip is derived
    /// from `feasible`.
    pub allowed: Vec<Operator>,
    /// Index of the currently keyboard-focused tab (for Tab / arrow navigation + highlight).
    pub selected_idx: RwSignal<usize>,
    /// Called with the chosen `(operator, target)` to commit the cage. `target`
    /// is `None` in With-Solution mode and `Some` in Without-Solution mode.
    pub on_commit: Callback<(Operator, Option<Target>)>,
    /// Without-Solution feasibility state. `None` selects With-Solution rendering.
    pub feasible: Option<RwSignal<FeasibilityState>>,
    /// Without-Solution: the operator whose target sub-picker is open.
    pub picked_operator: RwSignal<Option<Operator>>,
}

/// Computes the target value for `op` applied to `polyomino`'s cells using the solution
/// values read from `partial_solution`. Returns `None` if any cell's values are not a singleton.
fn compute_target(
    polyomino: &Polyomino,
    op: &Operator,
    partial_solution: &PartialSolution,
) -> Option<u64> {
    let vals: Vec<u64> = polyomino
        .cells()
        .iter()
        .map(|&cell| {
            let v = partial_solution.cell_value_singleton(cell)?;
            Some(u64::from(v))
        })
        .collect::<Option<Vec<_>>>()?;

    Some(match op {
        Operator::Given => vals[0],
        Operator::Add => vals.iter().sum(),
        Operator::Multiply => vals.iter().product(),
        Operator::Subtract => {
            let a = vals[0];
            let b = vals[1];
            a.abs_diff(b)
        }
        Operator::Divide => {
            let hi = vals[0].max(vals[1]);
            let lo = vals[0].min(vals[1]);
            if lo == 0 || !hi.is_multiple_of(lo) {
                return None;
            }
            hi / lo
        }
    })
}

/// Handles shortcut keys for the operation selector.
/// Returns `Some(operator)` if a key maps to an allowed operator, `None` otherwise.
pub fn key_to_operator(key: &str, allowed: &[Operator]) -> Option<Operator> {
    let op = match key {
        "+" => Some(Operator::Add),
        "-" => Some(Operator::Subtract),
        "x" | "X" => Some(Operator::Multiply),
        "/" => Some(Operator::Divide),
        _ => None,
    }?;
    allowed.contains(&op).then_some(op)
}

/// Clears the pending commit and removes the provisional cage from `designer_state`.
fn cancel_pending(
    pending: &PendingCommit,
    pending_commit: RwSignal<Option<PendingCommit>>,
    designer_state: RwSignal<mathdoku_designer_shared::State>,
    on_state_change: Callback<mathdoku_designer_shared::State>,
) {
    pending_commit.set(None);
    let poly = pending.polyomino.clone();
    let mut new_st = designer_state.get_untracked();
    let _ = new_st.provisional_cages.remove(&poly);
    on_state_change.run(new_st.clone());
    designer_state.set(new_st);
}

/// Keyboard handling for the Without-Solution two-step picker (operator strip,
/// then target list). Both steps support focus highlighting, Tab / arrow
/// navigation, Enter, and operator shortcut keys. All keys are consumed by the
/// caller so they don't leak to grid navigation behind the picker.
#[allow(clippy::too_many_arguments)]
fn handle_key_without_solution(
    key: &str,
    shift: bool,
    pending: &PendingCommit,
    feasible: RwSignal<FeasibilityState>,
    pending_commit: RwSignal<Option<PendingCommit>>,
    designer_state: RwSignal<mathdoku_designer_shared::State>,
    on_state_change: Callback<mathdoku_designer_shared::State>,
) {
    use crate::keys::{ARROW_LEFT, ARROW_RIGHT, ENTER, ESCAPE, TAB};

    let FeasibilityState::Ready(pairs) = feasible.get_untracked() else {
        // Still computing: nothing to navigate yet; only Escape cancels.
        if key == ESCAPE {
            cancel_pending(pending, pending_commit, designer_state, on_state_change);
        }
        return;
    };
    if pairs.is_empty() {
        if key == ESCAPE {
            cancel_pending(pending, pending_commit, designer_state, on_state_change);
        }
        return;
    }

    // Cyclic move of `selected_idx` over `n` items (forwards, or backwards when `back`).
    let step = |back: bool, n: usize| {
        let cur = pending.selected_idx.get_untracked();
        let next = if back {
            cur.saturating_add(n - 1) % n
        } else {
            (cur + 1) % n
        };
        pending.selected_idx.set(next);
    };

    match pending.picked_operator.get_untracked() {
        // Step one: operator strip (horizontal). Enter or a shortcut key opens
        // the chosen operator's target list.
        None => {
            let ops: Vec<Operator> = group_by_operator(&pairs)
                .into_iter()
                .map(|(op, _)| op)
                .collect();
            match key {
                ESCAPE => {
                    cancel_pending(pending, pending_commit, designer_state, on_state_change);
                }
                TAB | ARROW_RIGHT | ARROW_LEFT => step(shift || key == ARROW_LEFT, ops.len()),
                ENTER => {
                    if let Some(op) = ops.get(pending.selected_idx.get_untracked()) {
                        pending.picked_operator.set(Some(op.clone()));
                        pending.selected_idx.set(0);
                    }
                }
                key_str => {
                    if let Some(op) = key_to_operator(key_str, &ops) {
                        pending.picked_operator.set(Some(op));
                        pending.selected_idx.set(0);
                    }
                }
            }
        }
        // Step two: the native target dropdown (`target_select_view`) owns arrow,
        // type-ahead, and Enter navigation while focused, so only Escape reaches
        // here. For multi-cell cages it backs out to the operator strip; for
        // singletons (which open straight on the dropdown with no strip behind it)
        // it cancels the commit outright.
        Some(_) => {
            if key == ESCAPE {
                if pending.polyomino.len() == 1 {
                    cancel_pending(pending, pending_commit, designer_state, on_state_change);
                } else {
                    pending.picked_operator.set(None);
                    pending.selected_idx.set(0);
                }
            }
        }
    }
}

/// Handles keyboard events while the operation selector is active.
/// Returns `true` if the key was consumed.
pub fn handle_key(
    key: &str,
    shift: bool,
    pending: &PendingCommit,
    pending_commit: RwSignal<Option<PendingCommit>>,
    designer_state: RwSignal<mathdoku_designer_shared::State>,
    on_state_change: Callback<mathdoku_designer_shared::State>,
) -> bool {
    use crate::keys::{ARROW_LEFT, ARROW_RIGHT, ENTER, ESCAPE, TAB};

    // Without-Solution mode is a two-step picker handled separately.
    if let Some(feasible) = pending.feasible {
        handle_key_without_solution(
            key,
            shift,
            pending,
            feasible,
            pending_commit,
            designer_state,
            on_state_change,
        );
        return true;
    }

    let n_ops = pending.allowed.len();
    match key {
        ESCAPE => {
            cancel_pending(pending, pending_commit, designer_state, on_state_change);
            true
        }
        TAB | ARROW_RIGHT => {
            let next = if shift || key == ARROW_LEFT {
                pending
                    .selected_idx
                    .get_untracked()
                    .saturating_add(n_ops - 1)
                    % n_ops
            } else {
                (pending.selected_idx.get_untracked() + 1) % n_ops
            };
            pending.selected_idx.set(next);
            true
        }
        ARROW_LEFT => {
            pending.selected_idx.set(
                pending
                    .selected_idx
                    .get_untracked()
                    .saturating_add(n_ops - 1)
                    % n_ops,
            );
            true
        }
        ENTER => {
            let op = pending.allowed[pending.selected_idx.get_untracked()].clone();
            pending.on_commit.run((op, None));
            true
        }
        key_str => key_to_operator(key_str, &pending.allowed).is_some_and(|op| {
            pending.on_commit.run((op, None));
            true
        }),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{compute_target, key_to_operator};
    use crate::partial_solution::PartialSolution;
    use mathdoku::{Cell, Grid, Operator, Polyomino, Puzzle};

    fn poly(positions: &[(usize, usize)]) -> Polyomino {
        let cells: Vec<Cell> = positions.iter().map(|&(r, c)| Cell::new(r, c)).collect();
        Polyomino::from_cells(&cells).unwrap()
    }

    /// A `PartialSolution` whose grid pins every cell to the Latin square
    /// ```text
    /// 1 2 3
    /// 2 3 1
    /// 3 1 2
    /// ```
    fn pinned_3x3() -> PartialSolution {
        let square = vec![vec![1u8, 2, 3], vec![2, 3, 1], vec![3, 1, 2]];
        let grid = Grid::from_latin_square(3, &square).unwrap();
        PartialSolution::new(Puzzle::new(3).unwrap(), grid)
    }

    #[test]
    fn compute_target_given_is_first_cell_value() {
        let ps = pinned_3x3();
        assert_eq!(
            compute_target(&poly(&[(0, 1)]), &Operator::Given, &ps),
            Some(2)
        );
    }

    #[test]
    fn compute_target_add_sums_cells() {
        let ps = pinned_3x3();
        // (0,0)=1, (0,1)=2, (0,2)=3 → 6
        assert_eq!(
            compute_target(&poly(&[(0, 0), (0, 1), (0, 2)]), &Operator::Add, &ps),
            Some(6)
        );
    }

    #[test]
    fn compute_target_multiply_products_cells() {
        let ps = pinned_3x3();
        // (1,0)=2, (1,1)=3 → 6
        assert_eq!(
            compute_target(&poly(&[(1, 0), (1, 1)]), &Operator::Multiply, &ps),
            Some(6)
        );
    }

    #[test]
    fn compute_target_subtract_is_absolute_difference() {
        let ps = pinned_3x3();
        // (0,0)=1, (0,1)=2 → |1-2| = 1
        assert_eq!(
            compute_target(&poly(&[(0, 0), (0, 1)]), &Operator::Subtract, &ps),
            Some(1)
        );
    }

    #[test]
    fn compute_target_divide_returns_quotient_when_divisible() {
        // A 6×6 grid pinning (0,0)=2 and (0,1)=6 so that 6 / 2 = 3.
        // `from_latin_square` validates only the value range and dimensions, so
        // the remaining filler values need not form a real Latin square.
        let square = vec![
            vec![2u8, 6, 1, 3, 4, 5],
            vec![1, 2, 3, 4, 5, 6],
            vec![3, 4, 5, 6, 1, 2],
            vec![4, 5, 6, 1, 2, 3],
            vec![5, 6, 1, 2, 3, 4],
            vec![6, 1, 2, 3, 4, 5],
        ];
        let grid = Grid::from_latin_square(6, &square).unwrap();
        let ps = PartialSolution::new(Puzzle::new(6).unwrap(), grid);
        // (0,0)=2, (0,1)=6 → 6/2 = 3
        assert_eq!(
            compute_target(&poly(&[(0, 0), (0, 1)]), &Operator::Divide, &ps),
            Some(3)
        );
    }

    #[test]
    fn compute_target_divide_none_when_not_divisible() {
        let ps = pinned_3x3();
        // (0,1)=2, (0,2)=3 → 3 not divisible by 2 → None
        assert_eq!(
            compute_target(&poly(&[(0, 1), (0, 2)]), &Operator::Divide, &ps),
            None
        );
    }

    #[test]
    fn compute_target_none_when_values_not_singleton() {
        // Unconstrained grid: every cell's values are {1,2,3}, not a singleton.
        let ps = PartialSolution::new(Puzzle::new(3).unwrap(), Grid::new(3).unwrap());
        assert_eq!(
            compute_target(&poly(&[(0, 0)]), &Operator::Given, &ps),
            None
        );
    }

    #[test]
    fn key_to_operator_maps_known_keys() {
        let all = [
            Operator::Add,
            Operator::Subtract,
            Operator::Multiply,
            Operator::Divide,
        ];
        assert_eq!(key_to_operator("+", &all), Some(Operator::Add));
        assert_eq!(key_to_operator("-", &all), Some(Operator::Subtract));
        assert_eq!(key_to_operator("x", &all), Some(Operator::Multiply));
        assert_eq!(key_to_operator("X", &all), Some(Operator::Multiply));
        assert_eq!(key_to_operator("/", &all), Some(Operator::Divide));
    }

    #[test]
    fn key_to_operator_unknown_key_is_none() {
        let all = [Operator::Add, Operator::Multiply];
        assert_eq!(key_to_operator("q", &all), None);
        assert_eq!(key_to_operator("", &all), None);
    }

    #[test]
    fn key_to_operator_none_when_not_allowed() {
        // Subtract/Divide are not in the allowed list for this polyomino size.
        let allowed = [Operator::Add, Operator::Multiply];
        assert_eq!(key_to_operator("-", &allowed), None);
        assert_eq!(key_to_operator("/", &allowed), None);
        assert_eq!(key_to_operator("+", &allowed), Some(Operator::Add));
    }

    mod handle_key {
        use super::super::{PendingCommit, handle_key};
        use super::poly;
        use crate::keys::{ARROW_LEFT, ARROW_RIGHT, ENTER, ESCAPE, TAB};
        use leptos::prelude::*;
        use leptos::reactive::owner::Owner;
        use mathdoku::{Operator, Target};
        use mathdoku_designer_shared::State;

        const ALL_OPS: [Operator; 4] = [
            Operator::Add,
            Operator::Subtract,
            Operator::Multiply,
            Operator::Divide,
        ];

        type Committed = RwSignal<Option<(Operator, Option<Target>)>>;

        fn pending(committed: Committed) -> PendingCommit {
            let on_commit =
                Callback::new(move |pair: (Operator, Option<Target>)| committed.set(Some(pair)));
            PendingCommit {
                polyomino: poly(&[(0, 0), (0, 1)]),
                allowed: ALL_OPS.to_vec(),
                selected_idx: RwSignal::new(0usize),
                on_commit,
                feasible: None,
                picked_operator: RwSignal::new(None),
            }
        }

        #[test]
        fn escape_clears_pending_and_removes_provisional_cage() {
            Owner::new().with(|| {
                let committed = RwSignal::new(None);
                let p = pending(committed);

                let mut st = State::new(4).unwrap();
                let _ = st.provisional_cages.insert(p.polyomino.clone());
                let designer_state = RwSignal::new(st);
                let pending_commit = RwSignal::new(Some(p.clone()));
                let on_state_change = Callback::new(|_: State| {});

                let consumed = handle_key(
                    ESCAPE,
                    false,
                    &p,
                    pending_commit,
                    designer_state,
                    on_state_change,
                );

                assert!(consumed);
                assert!(pending_commit.get_untracked().is_none());
                assert!(
                    designer_state.get_untracked().provisional_cages.is_empty(),
                    "the provisional cage should have been removed"
                );
            });
        }

        #[test]
        fn tab_advances_selected_index() {
            Owner::new().with(|| {
                let committed = RwSignal::new(None);
                let p = pending(committed);
                let designer_state = RwSignal::new(State::new(4).unwrap());
                let pending_commit = RwSignal::new(Some(p.clone()));
                let on_state_change = Callback::new(|_: State| {});

                let consumed = handle_key(
                    TAB,
                    false,
                    &p,
                    pending_commit,
                    designer_state,
                    on_state_change,
                );

                assert!(consumed);
                assert_eq!(p.selected_idx.get_untracked(), 1);
            });
        }

        #[test]
        fn shift_tab_wraps_backwards() {
            Owner::new().with(|| {
                let committed = RwSignal::new(None);
                let p = pending(committed);
                p.selected_idx.set(0);
                let designer_state = RwSignal::new(State::new(4).unwrap());
                let pending_commit = RwSignal::new(Some(p.clone()));
                let on_state_change = Callback::new(|_: State| {});

                let _ = handle_key(
                    TAB,
                    true,
                    &p,
                    pending_commit,
                    designer_state,
                    on_state_change,
                );

                // From 0, Shift+Tab wraps to the last index (4 operators → 3).
                assert_eq!(p.selected_idx.get_untracked(), 3);
            });
        }

        #[test]
        fn arrow_right_advances_and_arrow_left_wraps() {
            Owner::new().with(|| {
                let committed = RwSignal::new(None);
                let p = pending(committed);
                let designer_state = RwSignal::new(State::new(4).unwrap());
                let pending_commit = RwSignal::new(Some(p.clone()));
                let on_state_change = Callback::new(|_: State| {});

                let _ = handle_key(
                    ARROW_RIGHT,
                    false,
                    &p,
                    pending_commit,
                    designer_state,
                    on_state_change,
                );
                assert_eq!(p.selected_idx.get_untracked(), 1);

                // ArrowLeft from index 1 moves back to 0.
                let _ = handle_key(
                    ARROW_LEFT,
                    false,
                    &p,
                    pending_commit,
                    designer_state,
                    on_state_change,
                );
                assert_eq!(p.selected_idx.get_untracked(), 0);
            });
        }

        #[test]
        fn enter_commits_the_selected_operator() {
            Owner::new().with(|| {
                let committed = RwSignal::new(None);
                let p = pending(committed);
                p.selected_idx.set(2); // Multiply
                let designer_state = RwSignal::new(State::new(4).unwrap());
                let pending_commit = RwSignal::new(Some(p.clone()));
                let on_state_change = Callback::new(|_: State| {});

                let consumed = handle_key(
                    ENTER,
                    false,
                    &p,
                    pending_commit,
                    designer_state,
                    on_state_change,
                );

                assert!(consumed);
                assert_eq!(committed.get_untracked(), Some((Operator::Multiply, None)));
            });
        }

        #[test]
        fn operator_shortcut_key_commits() {
            Owner::new().with(|| {
                let committed = RwSignal::new(None);
                let p = pending(committed);
                let designer_state = RwSignal::new(State::new(4).unwrap());
                let pending_commit = RwSignal::new(Some(p.clone()));
                let on_state_change = Callback::new(|_: State| {});

                let consumed = handle_key(
                    "+",
                    false,
                    &p,
                    pending_commit,
                    designer_state,
                    on_state_change,
                );

                assert!(consumed);
                assert_eq!(committed.get_untracked(), Some((Operator::Add, None)));
            });
        }

        #[test]
        fn unhandled_key_is_not_consumed() {
            Owner::new().with(|| {
                let committed = RwSignal::new(None);
                let p = pending(committed);
                let designer_state = RwSignal::new(State::new(4).unwrap());
                let pending_commit = RwSignal::new(Some(p.clone()));
                let on_state_change = Callback::new(|_: State| {});

                let consumed = handle_key(
                    "q",
                    false,
                    &p,
                    pending_commit,
                    designer_state,
                    on_state_change,
                );

                assert!(!consumed);
                assert!(committed.get_untracked().is_none());
            });
        }

        use super::super::FeasibilityState;

        fn ready_pending(committed: Committed, pairs: Vec<(Operator, Target)>) -> PendingCommit {
            let mut p = pending(committed);
            p.feasible = Some(RwSignal::new(FeasibilityState::Ready(pairs)));
            p
        }

        #[test]
        fn without_solution_strip_tab_navigates_and_enter_picks() {
            Owner::new().with(|| {
                let committed = RwSignal::new(None);
                let pairs: Vec<(Operator, Target)> =
                    vec![(Operator::Add, 3), (Operator::Subtract, 1)];
                let p = ready_pending(committed, pairs);
                let designer_state = RwSignal::new(State::new(4).unwrap());
                let pending_commit = RwSignal::new(Some(p.clone()));
                let on_state_change = Callback::new(|_: State| {});

                // Tab advances the highlighted operator.
                assert!(handle_key(
                    TAB,
                    false,
                    &p,
                    pending_commit,
                    designer_state,
                    on_state_change,
                ));
                assert_eq!(p.selected_idx.get_untracked(), 1);

                // Enter opens the highlighted operator's target list and resets the
                // index for the target sub-picker.
                assert!(handle_key(
                    ENTER,
                    false,
                    &p,
                    pending_commit,
                    designer_state,
                    on_state_change,
                ));
                assert_eq!(p.picked_operator.get_untracked(), Some(Operator::Subtract));
                assert_eq!(p.selected_idx.get_untracked(), 0);
                assert!(committed.get_untracked().is_none());
            });
        }

        #[test]
        fn without_solution_shortcut_key_picks_operator() {
            Owner::new().with(|| {
                let committed = RwSignal::new(None);
                let pairs: Vec<(Operator, Target)> =
                    vec![(Operator::Add, 3), (Operator::Subtract, 1)];
                let p = ready_pending(committed, pairs);
                let designer_state = RwSignal::new(State::new(4).unwrap());
                let pending_commit = RwSignal::new(Some(p.clone()));
                let on_state_change = Callback::new(|_: State| {});

                // "-" jumps straight to the Subtract target list without committing.
                assert!(handle_key(
                    "-",
                    false,
                    &p,
                    pending_commit,
                    designer_state,
                    on_state_change,
                ));
                assert_eq!(p.picked_operator.get_untracked(), Some(Operator::Subtract));
                assert!(committed.get_untracked().is_none());
            });
        }

        #[test]
        fn without_solution_escape_backs_out_then_cancels() {
            Owner::new().with(|| {
                let committed = RwSignal::new(None);
                let pairs: Vec<(Operator, Target)> = vec![(Operator::Add, 3)];
                let p = ready_pending(committed, pairs);
                p.picked_operator.set(Some(Operator::Add));

                let mut st = State::new(4).unwrap();
                let _ = st.provisional_cages.insert(p.polyomino.clone());
                let designer_state = RwSignal::new(st);
                let pending_commit = RwSignal::new(Some(p.clone()));
                let on_state_change = Callback::new(|_: State| {});

                // Escape from the target sub-picker backs out to the operator strip.
                assert!(handle_key(
                    ESCAPE,
                    false,
                    &p,
                    pending_commit,
                    designer_state,
                    on_state_change,
                ));
                assert!(p.picked_operator.get_untracked().is_none());
                assert!(pending_commit.get_untracked().is_some());

                // Escape again (no operator picked) cancels the pending commit.
                assert!(handle_key(
                    ESCAPE,
                    false,
                    &p,
                    pending_commit,
                    designer_state,
                    on_state_change,
                ));
                assert!(pending_commit.get_untracked().is_none());
            });
        }

        #[test]
        fn without_solution_computing_consumes_keys_until_ready() {
            Owner::new().with(|| {
                let committed = RwSignal::new(None);
                let mut p = pending(committed);
                p.feasible = Some(RwSignal::new(FeasibilityState::Computing));
                let designer_state = RwSignal::new(State::new(4).unwrap());
                let pending_commit = RwSignal::new(Some(p.clone()));
                let on_state_change = Callback::new(|_: State| {});

                // While computing, navigation keys are consumed but do nothing.
                assert!(handle_key(
                    "+",
                    false,
                    &p,
                    pending_commit,
                    designer_state,
                    on_state_change,
                ));
                assert!(committed.get_untracked().is_none());
                assert!(p.picked_operator.get_untracked().is_none());
            });
        }

        #[test]
        fn without_solution_singleton_escape_cancels_instead_of_backing_out() {
            Owner::new().with(|| {
                let committed = RwSignal::new(None);
                let on_commit = Callback::new(move |pair: (Operator, Option<Target>)| {
                    committed.set(Some(pair));
                });
                // A singleton opens straight on the value dropdown (picked = Given).
                let p = PendingCommit {
                    polyomino: poly(&[(0, 0)]),
                    allowed: vec![Operator::Given],
                    selected_idx: RwSignal::new(0usize),
                    on_commit,
                    feasible: Some(RwSignal::new(FeasibilityState::Ready(vec![
                        (Operator::Given, 1),
                        (Operator::Given, 2),
                    ]))),
                    picked_operator: RwSignal::new(Some(Operator::Given)),
                };

                let mut st = State::new(4).unwrap();
                let _ = st.provisional_cages.insert(p.polyomino.clone());
                let designer_state = RwSignal::new(st);
                let pending_commit = RwSignal::new(Some(p.clone()));
                let on_state_change = Callback::new(|_: State| {});

                assert!(handle_key(
                    ESCAPE,
                    false,
                    &p,
                    pending_commit,
                    designer_state,
                    on_state_change,
                ));
                // No operator strip to fall back to — the commit is cancelled.
                assert!(pending_commit.get_untracked().is_none());
                assert!(designer_state.get_untracked().provisional_cages.is_empty());
                assert!(committed.get_untracked().is_none());
            });
        }
    }
}
