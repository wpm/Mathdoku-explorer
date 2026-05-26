use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use mathdoku::Puzzle;
use mathdoku::{Cell, Operation, Operator, Polyomino};
use mathdoku_designer_shared::{DocState, ViewState};
use tauri::{AppHandle, Manager, Runtime, State};

pub const SAVE_VERSION: u32 = 1;
pub const RECENT_FILE: &str = "last_open.json";

#[derive(Default)]
pub struct AppState {
    pub puzzle: Option<Puzzle>,
    pub path: Option<String>,
    pub dirty: bool,
    pub view_state: ViewState,
}

#[derive(Serialize, Deserialize)]
pub struct SaveEnvelope {
    pub version: u32,
    pub puzzle: Puzzle,
}

#[derive(Serialize)]
pub struct SaveResult {
    pub path: String,
}

// ---- recent-file helpers ----

pub fn recent_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(RECENT_FILE))
}

pub fn write_recent<R: Runtime>(app: &AppHandle<R>, path: Option<&str>, view: &ViewState) {
    #[derive(Serialize)]
    struct Record<'a> {
        path: Option<&'a str>,
        view: &'a ViewState,
    }
    let Some(file) = recent_path(app) else { return };
    match path {
        Some(p) => {
            if let Some(parent) = file.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string(&Record {
                path: Some(p),
                view,
            }) {
                let _ = std::fs::write(file, json);
            }
        }
        None => {
            let _ = std::fs::remove_file(file);
        }
    }
}

#[derive(Deserialize)]
pub struct RecentRecord {
    pub path: Option<String>,
    #[serde(default)]
    pub view: Option<ViewState>,
}

pub fn read_recent<R: Runtime>(app: &AppHandle<R>) -> Option<RecentRecord> {
    let file = recent_path(app)?;
    let content = std::fs::read_to_string(file).ok()?;
    serde_json::from_str::<RecentRecord>(&content).ok()
}

// ---- commands ----

/// # Errors
/// Returns an error string if `n` is invalid or the state lock is poisoned.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn new_puzzle(n: usize, state: State<Mutex<AppState>>) -> Result<Puzzle, String> {
    let puzzle = Puzzle::new(n).map_err(|e| e.to_string())?;
    let mut s = state.lock().map_err(|e| e.to_string())?;
    s.puzzle = Some(puzzle.clone());
    s.path = None;
    s.dirty = true;
    drop(s);
    Ok(puzzle)
}

/// # Errors
/// Returns an error string if `n` is invalid or the state lock is poisoned.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn generate_puzzle(n: usize, state: State<Mutex<AppState>>) -> Result<Puzzle, String> {
    let puzzle = mathdoku::generate::generate(n, &mut rand::rng()).map_err(|e| e.to_string())?;
    let mut s = state.lock().map_err(|e| e.to_string())?;
    s.puzzle = Some(puzzle.clone());
    s.path = None;
    s.dirty = true;
    drop(s);
    Ok(puzzle)
}

/// # Errors
/// Returns an error string if no puzzle is loaded, serialization fails, or the file cannot be
/// written.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn save_puzzle<R: Runtime>(
    path: String,
    app: AppHandle<R>,
    state: State<Mutex<AppState>>,
) -> Result<SaveResult, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let puzzle = s.puzzle.as_ref().ok_or("no puzzle loaded")?.clone();
    let envelope = SaveEnvelope {
        version: SAVE_VERSION,
        puzzle,
    };
    let json = serde_json::to_string_pretty(&envelope).map_err(|e| e.to_string())?;
    std::fs::write(&path, &json).map_err(|e| e.to_string())?;
    s.path = Some(path.clone());
    s.dirty = false;
    let view = s.view_state.clone();
    drop(s);
    write_recent(&app, Some(&path), &view);
    Ok(SaveResult { path })
}

/// # Errors
/// Returns an error string if the file cannot be read, JSON is malformed, or the version is
/// unsupported.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn load_puzzle<R: Runtime>(
    path: String,
    app: AppHandle<R>,
    state: State<Mutex<AppState>>,
) -> Result<Puzzle, String> {
    let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let envelope: SaveEnvelope = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    if envelope.version != SAVE_VERSION {
        return Err(format!("unsupported version: {}", envelope.version));
    }
    let mut s = state.lock().map_err(|e| e.to_string())?;
    s.puzzle = Some(envelope.puzzle.clone());
    s.path = Some(path.clone());
    s.dirty = false;
    s.view_state = ViewState::default();
    let view = s.view_state.clone();
    drop(s);
    write_recent(&app, Some(&path), &view);
    Ok(envelope.puzzle)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn get_doc_state(state: State<Mutex<AppState>>) -> DocState {
    let s = state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    DocState {
        dirty: s.dirty,
        path: s.path.clone(),
    }
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
#[must_use]
pub fn get_puzzle(state: State<Mutex<AppState>>) -> Option<Puzzle> {
    state.lock().ok()?.puzzle.clone()
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
#[must_use]
pub fn get_view_state(state: State<Mutex<AppState>>) -> ViewState {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .view_state
        .clone()
}

/// # Errors
/// Returns an error string if the state lock is poisoned.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn set_view_state<R: Runtime>(
    view: ViewState,
    app: AppHandle<R>,
    state: State<Mutex<AppState>>,
) -> Result<(), String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    s.view_state = view;
    let path = s.path.clone();
    let view = s.view_state.clone();
    drop(s);
    write_recent(&app, path.as_deref(), &view);
    Ok(())
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn quit_app<R: Runtime>(app: AppHandle<R>) {
    app.exit(0);
}

/// # Errors
/// Returns an error string if no window is found or the title cannot be set.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn set_window_title<R: Runtime>(title: String, app: AppHandle<R>) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "no main window".to_string())?
        .set_title(&title)
        .map_err(|e| e.to_string())
}

/// Adds a cage to the current puzzle for the given cells.
///
/// `cells` is a list of `{row, column}` objects. Single-cell regions use `Given`
/// with target 1; multi-cell regions use `Add` with target 0 as a placeholder.
/// Returns the updated puzzle.
///
/// # Errors
/// Returns an error string if no puzzle is loaded or the cells form an invalid
/// polyomino.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn add_region(cells: Vec<Cell>, state: State<Mutex<AppState>>) -> Result<Puzzle, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let puzzle = s.puzzle.as_ref().ok_or("no puzzle loaded")?;
    let poly = Polyomino::from_cells(&cells).map_err(|e| e.to_string())?;
    let operation = if cells.len() == 1 {
        Operation::new(Operator::Given, 1)
    } else {
        Operation::new(Operator::Add, 0)
    };
    let cage = mathdoku::Cage::new(poly, operation);
    let new_puzzle = puzzle.insert_cage(cage).map_err(|e| e.to_string())?;
    s.puzzle = Some(new_puzzle.clone());
    s.dirty = true;
    drop(s);
    Ok(new_puzzle)
}

/// Removes the cage whose cell set matches `cells` from the current puzzle.
///
/// `cells` identifies the cage by its cell set. Returns the updated puzzle.
///
/// # Errors
/// Returns an error string if no puzzle is loaded, the cells form an invalid
/// polyomino, or no matching cage is found.
#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn remove_region(cells: Vec<Cell>, state: State<Mutex<AppState>>) -> Result<Puzzle, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let puzzle = s.puzzle.as_ref().ok_or("no puzzle loaded")?;
    let target_cells: std::collections::HashSet<_> = cells.iter().copied().collect();
    // Rebuild the puzzle without the matching cage.
    let n = puzzle.n();
    let remaining_cages: Vec<mathdoku::Cage> = puzzle
        .cages()
        .filter(|cage| {
            let cage_cells: std::collections::HashSet<_> = cage.cells().into_iter().collect();
            cage_cells != target_cells
        })
        .cloned()
        .collect();
    if remaining_cages.len() == puzzle.cages().count() {
        return Err("cage not found".to_string());
    }
    let new_puzzle = remaining_cages
        .into_iter()
        .try_fold(Puzzle::new(n).map_err(|e| e.to_string())?, |p, cage| {
            p.insert_cage(cage)
        })
        .map_err(|e| e.to_string())?;
    s.puzzle = Some(new_puzzle.clone());
    s.dirty = true;
    drop(s);
    Ok(new_puzzle)
}
