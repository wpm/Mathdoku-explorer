//! Centralized Tauri IPC surface.
//!
//! This module owns the single `wasm_bindgen` binding to Tauri's `invoke`
//! function and the dialog API, and exposes one typed Rust wrapper per Tauri
//! command. The rest of the frontend calls these wrappers instead of passing
//! stringly-typed command names and `JsValue` blobs around, so a renamed
//! command or a mismatched argument shape becomes a compile error rather than
//! a runtime failure. Reading this file gives the full IPC contract in one
//! place.

#![allow(
    clippy::future_not_send,         // WASM async is inherently single-threaded
    clippy::missing_errors_doc,      // every wrapper's error is "the Tauri command failed"
    clippy::unused_async,            // `web` bodies keep `async` to match the native signatures
    unused_results,                  // quit_app discards its fire-and-forget JsValue
)]

use mathdoku::{Cell, Operator, Polyomino, Target};
use mathdoku_designer_core::{DocState, SaveResult, State};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_wasm_bindgen::{from_value, to_value};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = "invoke")]
    async fn raw_invoke(cmd: &str, args: JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "dialog"], js_name = "open")]
    async fn dialog_open(options: JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "dialog"], js_name = "save")]
    async fn dialog_save(options: JsValue) -> JsValue;
}

/// An error crossing the Tauri IPC boundary.
#[derive(Debug, Clone)]
pub enum IpcError {
    /// The Tauri command ran but returned `Err(String)`.
    Command(String),
    /// Serializing the arguments or deserializing the response failed.
    Serde(String),
}

impl core::fmt::Display for IpcError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Command(msg) | Self::Serde(msg) => f.write_str(msg),
        }
    }
}

// ---- argument shapes ----

// The argument shapes below feed `raw_invoke`, so they exist only on the native
// (non-`web`) path. The `web` build calls core directly and passes these values
// as plain function arguments, so the ones used solely by migrated commands are
// gated out to avoid dead code there. `PathArgs` stays ungated: it backs the
// un-migrated save/load commands, which still use Tauri on both builds.

#[cfg(not(feature = "web"))]
#[derive(Serialize)]
struct NewPuzzleArgs {
    n: usize,
}

#[derive(Serialize)]
struct PathArgs {
    path: String,
}

#[cfg(not(feature = "web"))]
#[derive(Serialize)]
struct ActiveArgs {
    active: Cell,
}

#[cfg(not(feature = "web"))]
#[derive(Serialize)]
struct InsertCageArgs {
    polyomino: Polyomino,
    operator: Operator,
    /// `Some` in Without-Solution mode (author-chosen target); `None` in
    /// With-Solution mode (the backend derives the target from the solution).
    target: Option<Target>,
}

#[cfg(not(feature = "web"))]
#[derive(Serialize)]
struct RemoveCageAtArgs {
    polyomino: Polyomino,
}

#[cfg(not(feature = "web"))]
#[derive(Serialize)]
struct TitleArgs {
    title: String,
}

#[cfg(not(feature = "web"))]
#[derive(Serialize)]
struct MenuEnabledArgs {
    fix_enabled: bool,
    unfix_enabled: bool,
}

// ---- low-level call helpers ----

/// Detects the `Err(String)` arm of a Tauri command result.
///
/// Tauri serializes `Err(String)` as a plain JS string and `Ok(T)` as `T`'s
/// JSON. No command returns a bare string on success, so a string value here
/// unambiguously means the command failed.
fn command_error(value: &JsValue) -> Option<IpcError> {
    value.as_string().map(IpcError::Command)
}

/// Invokes a command whose Rust signature returns `Result<R, String>` and
/// deserializes the success payload into `R`.
async fn call<A, R>(cmd: &str, args: A) -> Result<R, IpcError>
where
    A: Serialize,
    R: DeserializeOwned,
{
    let args = to_value(&args).map_err(|e| IpcError::Serde(e.to_string()))?;
    let result = raw_invoke(cmd, args).await;
    if let Some(err) = command_error(&result) {
        return Err(err);
    }
    from_value(result).map_err(|e| IpcError::Serde(e.to_string()))
}

/// Invokes a command whose Rust signature returns `Result<(), String>`,
/// surfacing any command error but discarding the (null) success payload.
///
/// Only `set_active_cell` / `set_window_title` use this, and both are migrated
/// to direct calls on web, so it is dead on the `web` path.
#[cfg(not(feature = "web"))]
async fn call_unit<A: Serialize>(cmd: &str, args: A) -> Result<(), IpcError> {
    let args = to_value(&args).map_err(|e| IpcError::Serde(e.to_string()))?;
    let result = raw_invoke(cmd, args).await;
    command_error(&result).map_or(Ok(()), Err)
}

/// Invokes a no-argument command returning `Result<R, String>`.
///
/// Only `fix` / `unfix` used this, and both are migrated, so it is dead on the
/// `web` path.
#[cfg(not(feature = "web"))]
async fn call_no_args<R: DeserializeOwned>(cmd: &str) -> Result<R, IpcError> {
    let result = raw_invoke(cmd, JsValue::NULL).await;
    if let Some(err) = command_error(&result) {
        return Err(err);
    }
    from_value(result).map_err(|e| IpcError::Serde(e.to_string()))
}

// ---- command wrappers ----

/// Returns the document state, falling back to the default on any IPC error.
#[cfg(not(feature = "web"))]
pub async fn get_doc_state() -> DocState {
    let result = raw_invoke("get_doc_state", JsValue::NULL).await;
    from_value(result).unwrap_or_default()
}

/// Returns the document state, falling back to the default on any IPC error.
#[cfg(feature = "web")]
// WASM-only: no Tauri command bus on web — read the thread-local AppState directly.
pub async fn get_doc_state() -> DocState {
    crate::web_state::with_state(mathdoku_designer_core::get_doc_state)
}

/// Returns the restored designer state, or `None` if no puzzle is loaded.
#[cfg(not(feature = "web"))]
pub async fn get_puzzle() -> Option<State> {
    let result = raw_invoke("get_puzzle", JsValue::NULL).await;
    from_value(result).unwrap_or(None)
}

/// Returns the restored designer state, or `None` if no puzzle is loaded.
#[cfg(feature = "web")]
// WASM-only: no Tauri command bus on web — read the thread-local AppState directly.
pub async fn get_puzzle() -> Option<State> {
    crate::web_state::with_state(mathdoku_designer_core::get_puzzle)
}

#[cfg(not(feature = "web"))]
pub async fn new_latin_square(n: usize) -> Result<State, IpcError> {
    call("new_latin_square", NewPuzzleArgs { n }).await
}

#[cfg(feature = "web")]
// WASM-only: no Tauri command bus on web — run core against thread-local state,
// feeding it the web build's own RNG instead of the backend's.
pub async fn new_latin_square(n: usize) -> Result<State, IpcError> {
    crate::web_state::with_state_mut(|s| {
        mathdoku_designer_core::new_latin_square(s, n, &mut rand::rng())
    })
    .map_err(|e| IpcError::Command(e.to_string()))
}

#[cfg(not(feature = "web"))]
pub async fn new_empty(n: usize) -> Result<State, IpcError> {
    call("new_empty", NewPuzzleArgs { n }).await
}

#[cfg(feature = "web")]
// WASM-only: no Tauri command bus on web — call core directly against thread-local state.
pub async fn new_empty(n: usize) -> Result<State, IpcError> {
    crate::web_state::with_state_mut(|s| mathdoku_designer_core::new_empty(s, n))
        .map_err(|e| IpcError::Command(e.to_string()))
}

pub async fn save_puzzle(path: String) -> Result<SaveResult, IpcError> {
    call("save_puzzle", PathArgs { path }).await
}

pub async fn load_puzzle(path: String) -> Result<State, IpcError> {
    call("load_puzzle", PathArgs { path }).await
}

#[cfg(not(feature = "web"))]
pub async fn set_active_cell(active: Cell) -> Result<(), IpcError> {
    call_unit("set_active_cell", ActiveArgs { active }).await
}

#[cfg(feature = "web")]
// WASM-only: no Tauri command bus on web — mutate the thread-local AppState directly.
pub async fn set_active_cell(active: Cell) -> Result<(), IpcError> {
    crate::web_state::with_state_mut(|s| mathdoku_designer_core::set_active_cell(s, active));
    Ok(())
}

#[cfg(not(feature = "web"))]
pub async fn insert_cage(
    polyomino: Polyomino,
    operator: Operator,
    target: Option<Target>,
) -> Result<State, IpcError> {
    call(
        "insert_cage",
        InsertCageArgs {
            polyomino,
            operator,
            target,
        },
    )
    .await
}

#[cfg(feature = "web")]
// WASM-only: no Tauri command bus on web — call core directly against thread-local state.
pub async fn insert_cage(
    polyomino: Polyomino,
    operator: Operator,
    target: Option<Target>,
) -> Result<State, IpcError> {
    crate::web_state::with_state_mut(|s| {
        mathdoku_designer_core::insert_cage(s, polyomino, operator, target)
    })
    .map_err(|e| IpcError::Command(e.to_string()))
}

#[cfg(not(feature = "web"))]
pub async fn remove_cage_at(polyomino: Polyomino) -> Result<State, IpcError> {
    call("remove_cage_at", RemoveCageAtArgs { polyomino }).await
}

#[cfg(feature = "web")]
// WASM-only: no Tauri command bus on web — call core directly against thread-local state.
pub async fn remove_cage_at(polyomino: Polyomino) -> Result<State, IpcError> {
    crate::web_state::with_state_mut(|s| mathdoku_designer_core::remove_cage_at(s, &polyomino))
        .map_err(|e| IpcError::Command(e.to_string()))
}

/// Snapshots the unique completion into the solution (Without-Solution →
/// With-Solution). Errors if the puzzle does not have exactly one completion.
#[cfg(not(feature = "web"))]
pub async fn fix() -> Result<State, IpcError> {
    call_no_args("fix").await
}

/// Snapshots the unique completion into the solution (Without-Solution →
/// With-Solution). Errors if the puzzle does not have exactly one completion.
#[cfg(feature = "web")]
// WASM-only: no Tauri command bus on web — call core directly against thread-local state.
pub async fn fix() -> Result<State, IpcError> {
    crate::web_state::with_state_mut(mathdoku_designer_core::fix)
        .map_err(|e| IpcError::Command(e.to_string()))
}

/// Discards the solution (With-Solution → Without-Solution).
#[cfg(not(feature = "web"))]
pub async fn unfix() -> Result<State, IpcError> {
    call_no_args("unfix").await
}

/// Discards the solution (With-Solution → Without-Solution).
#[cfg(feature = "web")]
// WASM-only: no Tauri command bus on web — call core directly against thread-local state.
pub async fn unfix() -> Result<State, IpcError> {
    crate::web_state::with_state_mut(mathdoku_designer_core::unfix)
        .map_err(|e| IpcError::Command(e.to_string()))
}

#[cfg(not(feature = "web"))]
pub async fn set_window_title(title: String) -> Result<(), IpcError> {
    call_unit("set_window_title", TitleArgs { title }).await
}

/// Pushes the enabled state of the native Puzzle menu's Fix / Unfix items.
/// Exactly one is enabled at a time, mirroring the frontend's mode predicates.
#[cfg(not(feature = "web"))]
pub async fn set_puzzle_menu_enabled(
    fix_enabled: bool,
    unfix_enabled: bool,
) -> Result<(), IpcError> {
    call_unit(
        "set_puzzle_menu_enabled",
        MenuEnabledArgs {
            fix_enabled,
            unfix_enabled,
        },
    )
    .await
}

/// Web build: there is no native menu bar, so this is a no-op.
#[cfg(feature = "web")]
// WASM-only: no native menu to enable/disable on web.
pub async fn set_puzzle_menu_enabled(
    _fix_enabled: bool,
    _unfix_enabled: bool,
) -> Result<(), IpcError> {
    Ok(())
}

/// Sets the browser tab title.
#[cfg(feature = "web")]
// WASM-only: no Tauri window to title — write `document.title` directly.
pub async fn set_window_title(title: String) -> Result<(), IpcError> {
    crate::web_state::set_window_title(&title);
    Ok(())
}

/// Exits the application. Never returns meaningfully (the process is killed).
#[cfg(not(feature = "web"))]
pub async fn quit_app() {
    raw_invoke("quit_app", JsValue::NULL).await;
}

/// Web build: there is no application process to exit, so this is a no-op.
#[cfg(feature = "web")]
pub async fn quit_app() {}

// ---- file dialogs ----

#[derive(Serialize)]
struct FileFilter {
    name: String,
    extensions: Vec<String>,
}

#[derive(Serialize)]
struct DialogOptions {
    filters: Vec<FileFilter>,
}

fn mathdoku_dialog_options() -> DialogOptions {
    DialogOptions {
        filters: vec![FileFilter {
            name: "Mathdoku".to_owned(),
            extensions: vec!["mathdoku".to_owned()],
        }],
    }
}

/// Opens the native "open file" dialog, returning the chosen path or `None`
/// if the user cancelled.
pub async fn open_puzzle_dialog() -> Option<String> {
    let options = to_value(&mathdoku_dialog_options()).ok()?;
    dialog_open(options).await.as_string()
}

/// Opens the native "save file" dialog, returning the chosen path or `None`
/// if the user cancelled.
pub async fn save_puzzle_dialog() -> Option<String> {
    let options = to_value(&mathdoku_dialog_options()).ok()?;
    dialog_save(options).await.as_string()
}
