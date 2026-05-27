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
    fn compute_target_none_when_domain_not_singleton() {
        // Unconstrained grid: every cell domain is {1,2,3}, not a singleton.
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
        use mathdoku::Operator;
        use mathdoku_designer_shared::State;

        const ALL_OPS: [Operator; 4] = [
            Operator::Add,
            Operator::Subtract,
            Operator::Multiply,
            Operator::Divide,
        ];

        fn pending(committed: RwSignal<Option<Operator>>) -> PendingCommit {
            let on_commit = Callback::new(move |op: Operator| committed.set(Some(op)));
            PendingCommit {
                polyomino: poly(&[(0, 0), (0, 1)]),
                allowed: ALL_OPS.to_vec(),
                selected_idx: RwSignal::new(0usize),
                on_commit,
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
                assert_eq!(committed.get_untracked(), Some(Operator::Multiply));
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
                assert_eq!(committed.get_untracked(), Some(Operator::Add));
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
    }
}
