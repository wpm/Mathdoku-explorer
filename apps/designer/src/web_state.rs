//! WASM-only in-process application state for the web/preview build.
//!
//! This module exists **only** under `--features web` and has no native
//! analog — do not extend it for the Tauri target.
//!
//! On native, the backend owns the single [`AppState`] inside a Tauri-managed
//! `Mutex<AppState>` (`app.manage(Mutex::new(AppState::default()))`), and the
//! `#[tauri::command]` handlers lock it per call. A browser preview has no
//! Tauri runtime to hold that state, so instead we park one [`AppState`] in a
//! `thread_local!` `RefCell`. WASM is single-threaded, so a thread-local is
//! effectively a process global; the `RefCell` gives the interior mutability
//! that the `Mutex` provides on native, without any locking. State is
//! initialized lazily on first access (an empty [`AppState`] with no puzzle,
//! which makes `get_puzzle` return `None` at boot and surfaces the New-puzzle
//! size modal — matching the native fresh-start flow).
//!
//! The cfg-gated `#[cfg(feature = "web")]` bodies in [`crate::ipc`] reach their
//! state through [`with_state`] / [`with_state_mut`] and call the same
//! `mathdoku-designer-core` free functions the native command wrappers do.

use std::cell::RefCell;

use mathdoku_designer_core::AppState;

thread_local! {
    /// The single web-build [`AppState`], created on first access.
    static APP_STATE: RefCell<AppState> = RefCell::new(AppState::default());
}

/// Borrows the thread-local [`AppState`] immutably and runs `f` against it.
///
/// Used by the read-only web IPC bodies (`get_doc_state`, `get_puzzle`), which
/// clone their result out of the borrow.
pub fn with_state<R>(f: impl FnOnce(&AppState) -> R) -> R {
    APP_STATE.with(|cell| f(&cell.borrow()))
}

/// Borrows the thread-local [`AppState`] mutably and runs `f` against it.
///
/// Used by the mutating web IPC bodies (`new_empty`, `new_latin_square`,
/// `set_active_cell`, `insert_cage`, `remove_cage_at`, `fix`, `unfix`), which
/// hand `&mut AppState` straight to the matching core function.
pub fn with_state_mut<R>(f: impl FnOnce(&mut AppState) -> R) -> R {
    APP_STATE.with(|cell| f(&mut cell.borrow_mut()))
}

/// Sets the browser tab title via `document.title`.
///
/// The native build routes the equivalent through Tauri's `set_window_title`
/// command (`window.set_title`); the web build has no window to title, so it
/// writes the DOM directly. A missing `window`/`document` (impossible in a real
/// browser) is silently ignored.
pub fn set_window_title(title: &str) {
    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
        document.set_title(title);
    }
}

/// A short banner rendered above the canvas, making the ephemeral nature of the
/// web preview explicit: reloading the tab starts the visitor over (ADR-0002).
///
/// Invoked as `<EphemeralBanner />` through Leptos's `view!` macro, never for
/// its return value, so `must_use_candidate` does not apply.
#[allow(clippy::must_use_candidate)]
#[leptos::component]
pub fn EphemeralBanner() -> impl leptos::IntoView {
    use leptos::prelude::*;
    let style = "padding:6px 12px;background:#FFF7E6;border-bottom:0.5px solid #E5D8B8;\
                 color:#7A5C00;font-family:sans-serif;font-size:12.5px;text-align:center;";
    view! {
        <div style=style>
            "Ephemeral demo \u{2014} install the app to save what you make."
        </div>
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    //! Exercises the web-side store under a wasm test runner.
    //!
    //! These cover the three behaviors the store has to get right: lazy
    //! init-on-first-access, repeated `borrow_mut` across sequential `ipc::*`
    //! calls without aliasing the `RefCell`, and surviving a
    //! `serialize_save` / `apply_loaded` round trip.
    //!
    //! Each test re-seeds the puzzle through `ipc::new_empty` first, since the
    //! thread-local is shared across tests on the single wasm thread.

    use mathdoku::{Cell, Operator, Polyomino};
    use mathdoku_designer_core::{apply_loaded, serialize_save};
    use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

    use super::with_state;
    use crate::ipc;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn init_on_first_access_has_no_puzzle() {
        // A freshly-initialized store holds an empty `AppState`, so `get_doc_state`
        // reports the default (not dirty, no path) before any puzzle is created.
        let doc = with_state(mathdoku_designer_core::get_doc_state);
        assert!(doc.path.is_none());
    }

    #[wasm_bindgen_test]
    async fn sequential_ipc_calls_share_one_store() {
        // Each `ipc::*` call takes its own `borrow_mut`; running several in
        // sequence must not panic on an outstanding borrow and must accumulate
        // into the same `AppState`.
        let _ = ipc::new_empty(4).await.unwrap();
        ipc::set_active_cell(Cell::new(1, 2)).await.unwrap();
        let poly = Polyomino::from_cells(&[Cell::new(0, 0), Cell::new(0, 1)]).unwrap();
        let state = ipc::insert_cage(poly, Operator::Add, Some(3))
            .await
            .unwrap();
        assert_eq!(state.puzzle.cages().count(), 1);

        // The selection and the new cage are both visible through a later read.
        let puzzle = ipc::get_puzzle().await.unwrap();
        assert_eq!(puzzle.active, Cell::new(1, 2));
        assert_eq!(puzzle.puzzle.cages().count(), 1);
    }

    #[wasm_bindgen_test]
    async fn store_survives_save_load_round_trip() {
        let _ = ipc::new_empty(4).await.unwrap();
        let poly = Polyomino::from_cells(&[Cell::new(0, 0), Cell::new(0, 1)]).unwrap();
        let _ = ipc::insert_cage(poly, Operator::Add, Some(3))
            .await
            .unwrap();

        // Serialize out of the store, then load straight back into it.
        let json = with_state(|s| serialize_save(s).unwrap());
        let designer = super::with_state_mut(|s| apply_loaded(s, &json).unwrap());
        assert_eq!(designer.puzzle.cages().count(), 1);

        // The reloaded store is queryable through the normal read path.
        let puzzle = ipc::get_puzzle().await.unwrap();
        assert_eq!(puzzle.puzzle.cages().count(), 1);
    }
}
