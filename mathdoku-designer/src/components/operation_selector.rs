//! Operation selector: tab well shown in the anchor cell when the user is
//! choosing an operator for a pending cage commit.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]

use leptos::prelude::*;
use mathdoku::{Operation, Operator, Polyomino};

use super::puzzle::InteractionState;
use crate::geometry::{anchor, origin};
use crate::partial_solution::PartialSolution;
use crate::theme::{ACCENT, BG, INK, LINE, SERIF};

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

        // Singleton: only Given is allowed — commit immediately without showing UI.
        // (The Enter handler already handles this case, but guard here too.)
        if pending.allowed == [Operator::Given] {
            pending.on_commit.run(Operator::Given);
            return ().into_any();
        }

        let cell_size = ctx.cell_size;
        let a = anchor(&pending.polyomino.cells());
        let (x, y) = origin(cell_size, a.row, a.column);

        let tab_w = cell_size.clamp(44.0, 56.0);
        let tab_h = 28.0;
        let pad = 4.0;
        let gap = 2.0;
        let on_commit = pending.on_commit;
        let partial_solution = ctx.partial_solution.clone();
        let polyomino = pending.polyomino.clone();
        let selected_idx = pending.selected_idx;

        // Build (operator, label) pairs. When all cell domains are singletons,
        // omit any operator for which compute_target returns None (e.g. Divide
        // on non-divisible values). When domains are undetermined, show all
        // allowed operators with a label of just the operator symbol.
        let all_determined = polyomino
            .cells()
            .iter()
            .all(|&c| partial_solution.cell_value_singleton(c).is_some());
        let ops: Vec<(Operator, String)> = pending
            .allowed
            .iter()
            .filter_map(|op| {
                let target = compute_target(&polyomino, op, &partial_solution);
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
                // Background panel
                <rect
                    x={x} y={y}
                    width={total_w} height={total_h}
                    rx="4"
                    fill=BG
                    stroke=LINE
                    stroke-width="0.75"
                />
                // Operator tabs
                {ops.into_iter().enumerate().map(|(i, (op, label))| {
                    let tab_x = (tab_w + gap).mul_add(i as f64, x + pad);
                    let tab_y = y + pad;
                    let tx = tab_x + tab_w / 2.0;
                    let ty = tab_y + tab_h / 2.0;

                    view! {
                        <g
                            style="cursor:pointer;"
                            on:click=move |_| on_commit.run(op.clone())
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
}

/// The polyomino pending a cage commit and a callback invoked with the chosen operator.
#[derive(Clone)]
pub struct PendingCommit {
    /// The cells that will form the new cage.
    pub polyomino: Polyomino,
    /// The operators that are valid for this cage's polyomino size.
    pub allowed: Vec<Operator>,
    /// Index of the currently keyboard-focused tab (for Tab / arrow navigation + highlight).
    pub selected_idx: RwSignal<usize>,
    /// Called with the chosen operator to commit the cage.
    pub on_commit: Callback<Operator>,
}

/// Computes the target value for `op` applied to `polyomino`'s cells using the solution
/// values read from `partial_solution`. Returns `None` if any cell's domain is not a singleton.
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
    let n_ops = pending.allowed.len();
    match key {
        ESCAPE => {
            pending_commit.set(None);
            // Remove the provisional cage from state using the live designer_state signal.
            let poly = pending.polyomino.clone();
            let mut new_st = designer_state.get_untracked();
            let _ = new_st.provisional_cages.remove(&poly);
            on_state_change.run(new_st.clone());
            designer_state.set(new_st);
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
            pending.on_commit.run(op);
            true
        }
        key_str => key_to_operator(key_str, &pending.allowed).is_some_and(|op| {
            pending.on_commit.run(op);
            true
        }),
    }
}
