//! Cage IPC operations: commit a provisional cage, or demote a committed cage back to provisional.

#![allow(unused_results)]

use std::collections::BTreeSet;

use leptos::prelude::*;
use leptos::task::spawn_local;
use mathdoku::{Operator, Polyomino, Target};
use mathdoku_designer_core::State;

use crate::ipc;

/// Applies a successful IPC result to the designer state.
///
/// `parked` overrides the provisional cages in `new_st`; if `None`, the
/// pre-operation cages are preserved unchanged.
fn apply_ipc_result(
    mut new_st: State,
    parked: Option<BTreeSet<Polyomino>>,
    undo_stack: RwSignal<Vec<State>>,
    redo_stack: RwSignal<Vec<State>>,
    designer_state: RwSignal<State>,
    on_puzzle_change: Callback<State>,
) {
    let pre = designer_state.get_untracked();
    new_st.provisional_cages = parked.unwrap_or_else(|| pre.provisional_cages.clone());
    new_st.active = pre.active;
    undo_stack.update(|s| s.push(pre));
    redo_stack.update(Vec::clear);
    designer_state.set(new_st.clone());
    on_puzzle_change.run(new_st);
}

/// Commits `polyomino` as a new cage via the `insert_cage` Tauri command.
///
/// `target` is `None` in With-Solution mode (the backend derives the target
/// from the solution) and `Some` in Without-Solution mode (the author chose it).
///
/// On success, pushes the pre-commit state onto `undo_stack`, clears `redo_stack`,
/// restores `parked` provisional cages into the new state, and calls `on_puzzle_change`.
/// On IPC error, calls `on_error`.
#[allow(clippy::too_many_arguments)]
pub fn commit_cage(
    polyomino: &Polyomino,
    operator: Operator,
    target: Option<Target>,
    parked: BTreeSet<Polyomino>,
    undo_stack: RwSignal<Vec<State>>,
    redo_stack: RwSignal<Vec<State>>,
    designer_state: RwSignal<State>,
    on_puzzle_change: Callback<State>,
    on_error: Callback<String>,
) {
    let poly = polyomino.clone();
    spawn_local(async move {
        let new_st = match ipc::insert_cage(poly, operator, target).await {
            Ok(st) => st,
            Err(e) => {
                on_error.run(e.to_string());
                return;
            }
        };
        apply_ipc_result(
            new_st,
            Some(parked),
            undo_stack,
            redo_stack,
            designer_state,
            on_puzzle_change,
        );
    });
}

/// Deletes a committed cage outright via the `remove_cage_at` Tauri command.
///
/// Unlike [`demote_cage`], the removed cage is *not* re-added as a provisional
/// cage and no operation selector is opened — the cells become uncovered. On
/// success, pushes the pre-delete state onto `undo_stack`, clears `redo_stack`,
/// preserves the existing provisional cages and active cell, and calls
/// `on_puzzle_change`. On IPC error, calls `on_error`.
pub fn delete_cage(
    poly: Polyomino,
    undo_stack: RwSignal<Vec<State>>,
    redo_stack: RwSignal<Vec<State>>,
    designer_state: RwSignal<State>,
    on_puzzle_change: Callback<State>,
    on_error: Callback<String>,
) {
    spawn_local(async move {
        let new_st = match ipc::remove_cage_at(poly).await {
            Ok(st) => st,
            Err(e) => {
                on_error.run(e.to_string());
                return;
            }
        };
        apply_ipc_result(
            new_st,
            None,
            undo_stack,
            redo_stack,
            designer_state,
            on_puzzle_change,
        );
    });
}

/// Demotes a committed cage back to a provisional cage via the `remove_cage_at` Tauri command,
/// then signals the new Puzzle instance to open the operation selector for it.
///
/// On success, pushes the pre-demote state onto `undo_stack`, clears `redo_stack`, adds the
/// cage's polyomino to `provisional_cages`, and writes the polyomino into `pending_selector`
/// so the newly-mounted Puzzle can open its own selector for it. The signal must be app-level
/// (stable across Puzzle re-mounts); a Callback from the old Puzzle's scope would be disposed
/// before it could fire. On IPC error, calls `on_error`.
pub fn demote_cage(
    poly: Polyomino,
    undo_stack: RwSignal<Vec<State>>,
    redo_stack: RwSignal<Vec<State>>,
    designer_state: RwSignal<State>,
    on_puzzle_change: Callback<State>,
    pending_selector: RwSignal<Option<Polyomino>>,
    on_error: Callback<String>,
) {
    spawn_local(async move {
        let new_st = match ipc::remove_cage_at(poly.clone()).await {
            Ok(st) => st,
            Err(e) => {
                on_error.run(e.to_string());
                return;
            }
        };
        let mut parked = designer_state.get_untracked().provisional_cages;
        parked.insert(poly.clone());
        apply_ipc_result(
            new_st,
            Some(parked),
            undo_stack,
            redo_stack,
            designer_state,
            on_puzzle_change,
        );
        pending_selector.set(Some(poly));
    });
}
