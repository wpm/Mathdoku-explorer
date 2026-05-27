//! Cage IPC operations: commit a provisional cage, or demote a committed cage back to provisional.

#![allow(unused_results)]

use std::collections::BTreeSet;

use leptos::prelude::*;
use leptos::task::spawn_local;
use mathdoku::{Cell, Operator, Polyomino};
use mathdoku_designer_shared::State;

use crate::ipc;

/// Commits `polyomino` as a new cage via the `add_region` Tauri command.
///
/// On success, pushes the pre-commit state onto `undo_stack`, clears `redo_stack`,
/// restores `parked` provisional cages into the new state, and calls `on_puzzle_change`.
/// On IPC error, calls `on_error`.
#[allow(clippy::too_many_arguments)]
pub fn commit_cage(
    polyomino: &Polyomino,
    operator: Operator,
    parked: BTreeSet<Polyomino>,
    undo_stack: RwSignal<Vec<State>>,
    redo_stack: RwSignal<Vec<State>>,
    designer_state: RwSignal<State>,
    on_puzzle_change: Callback<State>,
    on_error: Callback<String>,
) {
    let cells = polyomino.cells();
    spawn_local(async move {
        let mut new_st = match ipc::add_region(cells, operator).await {
            Ok(st) => st,
            Err(e) => {
                on_error.run(e.to_string());
                return;
            }
        };
        let pre_commit = designer_state.get_untracked();
        // Restore parked provisional cages and active cell into the new state.
        new_st.provisional_cages = parked;
        new_st.active = pre_commit.active;
        undo_stack.update(|s| s.push(pre_commit));
        redo_stack.update(std::vec::Vec::clear);
        designer_state.set(new_st.clone());
        on_puzzle_change.run(new_st);
    });
}

/// Deletes a committed cage outright via the `remove_region` Tauri command.
///
/// Unlike [`demote_cage`], the removed cage is *not* re-added as a provisional
/// cage and no operation selector is opened — the cells become uncovered. On
/// success, pushes the pre-delete state onto `undo_stack`, clears `redo_stack`,
/// preserves the existing provisional cages and active cell, and calls
/// `on_puzzle_change`. On IPC error, calls `on_error`.
pub fn delete_cage(
    cells: Vec<Cell>,
    undo_stack: RwSignal<Vec<State>>,
    redo_stack: RwSignal<Vec<State>>,
    designer_state: RwSignal<State>,
    on_puzzle_change: Callback<State>,
    on_error: Callback<String>,
) {
    spawn_local(async move {
        let mut new_st = match ipc::remove_region(cells).await {
            Ok(st) => st,
            Err(e) => {
                on_error.run(e.to_string());
                return;
            }
        };
        let pre_delete = designer_state.get_untracked();
        new_st
            .provisional_cages
            .clone_from(&pre_delete.provisional_cages);
        new_st.active = pre_delete.active;
        undo_stack.update(|s| s.push(pre_delete));
        redo_stack.update(std::vec::Vec::clear);
        designer_state.set(new_st.clone());
        on_puzzle_change.run(new_st);
    });
}

/// Demotes a committed cage back to a provisional cage via the `remove_region` Tauri command,
/// then opens the operation selector for it.
///
/// On success, pushes the pre-demote state onto `undo_stack`, clears `redo_stack`, adds the
/// cage's polyomino to `provisional_cages`, and calls `on_open_selector` with the polyomino
/// so the caller can show the operation selector. On IPC error, calls `on_error`.
pub fn demote_cage(
    cells: Vec<Cell>,
    undo_stack: RwSignal<Vec<State>>,
    redo_stack: RwSignal<Vec<State>>,
    designer_state: RwSignal<State>,
    on_puzzle_change: Callback<State>,
    on_open_selector: Callback<Polyomino>,
    on_error: Callback<String>,
) {
    spawn_local(async move {
        let mut new_st = match ipc::remove_region(cells.clone()).await {
            Ok(st) => st,
            Err(e) => {
                on_error.run(e.to_string());
                return;
            }
        };
        let pre_demote = designer_state.get_untracked();
        // Add the demoted cage as a provisional cage in the new state.
        let Ok(poly) = Polyomino::from_cells(&cells) else {
            on_error.run("invalid polyomino".into());
            return;
        };
        new_st
            .provisional_cages
            .clone_from(&pre_demote.provisional_cages);
        new_st.provisional_cages.insert(poly.clone());
        new_st.active = pre_demote.active;
        undo_stack.update(|s| s.push(pre_demote));
        redo_stack.update(std::vec::Vec::clear);
        designer_state.set(new_st.clone());
        on_puzzle_change.run(new_st);
        on_open_selector.run(poly);
    });
}
