#![allow(
    clippy::needless_pass_by_value, // Tauri commands must take args by value
    clippy::must_use_candidate,     // Tauri handles return values via IPC
)]

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use serde_json::{from_str, to_string, to_string_pretty};

use mathdoku::{Cage, Cell, Grid, Operation, Operator, Polyomino, Puzzle, generate_latin_square};
use mathdoku_designer_shared::{DocState, SaveResult, State};
use tauri::{AppHandle, Manager, Runtime, State as TauriState};
/// Serialization version written into every `.mathdoku` save file.
/// Increment when the `SaveEnvelope` format changes in a breaking way.
pub const SAVE_VERSION: u32 = 1;

/// Filename of the recent-file record stored in the app data directory.
pub const RECENT_FILE: &str = "last_open.json";

/// Mutable backend state managed by Tauri as a `Mutex<AppState>`.
///
/// All fields are `None` until a puzzle is created or loaded.
/// `solution` and `current` are always kept in sync with `puzzle`
/// by every command that mutates the puzzle.
#[derive(Default)]
pub struct AppState {
    /// Cage structure being designed.
    pub puzzle: Option<Puzzle>,
    /// Latin-square solution fixed at puzzle creation. Singleton values for every cell.
    pub solution: Option<Grid>,
    /// Working grid: cell values constrained by the current cages against the solution.
    pub current: Option<Grid>,
    /// Path of the currently open `.mathdoku` file, or `None` if unsaved.
    pub path: Option<String>,
    /// Whether the puzzle has unsaved changes.
    pub dirty: bool,
    /// Last-known active cell, persisted in `last_open.json`.
    /// `None` means (0, 0) (default when no puzzle is loaded or after a fresh load).
    pub active: Option<Cell>,
}

impl AppState {
    /// Assembles a [`State`] from the current fields, or `None` if no puzzle is loaded.
    ///
    /// A loaded puzzle requires both `puzzle` and `current`; `solution` is `None`
    /// in Without-Solution mode and passes through unchanged.
    /// `provisional_cages` is always empty — provisional state lives only in the frontend.
    fn to_designer_state(&self) -> Option<State> {
        let puzzle = self.puzzle.clone()?;
        let current = self.current.clone()?;
        Some(State {
            puzzle,
            solution: self.solution.clone(),
            current,
            active: self.active.unwrap_or_else(|| Cell::new(0, 0)),
            provisional_cages: BTreeSet::new(),
        })
    }
}

/// On-disk format for `.mathdoku` save files.
///
/// `puzzle` (the cage structure) is always persisted. `solution` (the fixed
/// Latin square) is present only for With-Solution puzzles; Without-Solution
/// puzzles omit the field, which deserializes back to `None`. Existing
/// With-Solution save files carry a non-null `solution` and load unchanged.
#[derive(Serialize, Deserialize)]
pub struct SaveEnvelope {
    pub version: u32,
    pub puzzle: Puzzle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solution: Option<Grid>,
}

/// Computes the working `current` grid for `puzzle`.
///
/// In With-Solution mode the cages are re-applied to the fixed Latin square so
/// its singleton values are preserved. In Without-Solution mode there is no
/// solution to start from, so the cages are propagated from a fresh
/// unconstrained grid.
pub(crate) fn constrain_current(
    solution: Option<&Grid>,
    puzzle: &Puzzle,
) -> Result<Grid, mathdoku::Error> {
    match solution {
        Some(grid) => grid.constrain(puzzle),
        None => Grid::new(puzzle.n()).and_then(|g| g.constrain(puzzle)),
    }
}

// ---- recent-file helpers ----

/// Returns the path of the recent-file record (`last_open.json`) in the app data directory.
pub fn recent_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(RECENT_FILE))
}

/// Writes or removes the recent-file record.
///
/// `path = Some(p)` writes `{ path: p, active: … }` to `last_open.json`.
/// `path = None` deletes `last_open.json` so the next launch starts fresh.
pub fn write_recent<R: Runtime>(app: &AppHandle<R>, path: Option<&str>, active: Option<Cell>) {
    #[derive(Serialize)]
    struct Record<'a> {
        path: Option<&'a str>,
        active: Option<Cell>,
    }
    let Some(file) = recent_path(app) else { return };
    match path {
        Some(p) => {
            if let Some(parent) = file.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(json) = to_string(&Record {
                path: Some(p),
                active,
            }) {
                let _ = fs::write(file, json);
            }
        }
        None => {
            let _ = fs::remove_file(file);
        }
    }
}

/// Deserialized contents of `last_open.json`.
#[derive(Deserialize)]
pub struct RecentRecord {
    pub path: Option<String>,
    #[serde(default)]
    pub active: Option<Cell>,
}

/// Reads and parses `last_open.json`, returning `None` if the file is absent or malformed.
pub fn read_recent<R: Runtime>(app: &AppHandle<R>) -> Option<RecentRecord> {
    let file = recent_path(app)?;
    let content = fs::read_to_string(file).ok()?;
    from_str::<RecentRecord>(&content).ok()
}

// ---- commands ----

/// Creates a new empty *n*×*n* Without-Solution puzzle with no cages and no
/// Latin-square solution.
///
/// `solution` is `None`; the author chooses operator and target for each cage,
/// filtered by global feasibility. `current` starts as an unconstrained grid.
///
/// # Errors
/// Returns an error string if `n` is invalid or the state lock is poisoned.
#[tauri::command]
pub fn new_empty(n: usize, state: TauriState<Mutex<AppState>>) -> Result<State, String> {
    let puzzle = Puzzle::new(n).map_err(|e| e.to_string())?;
    let current = Grid::new(n)
        .and_then(|g| g.constrain(&puzzle))
        .map_err(|e| e.to_string())?;
    let mut s = state.lock().map_err(|e| e.to_string())?;
    s.puzzle = Some(puzzle);
    s.solution = None;
    s.current = Some(current);
    s.path = None;
    s.dirty = true;
    let designer_state = s.to_designer_state().ok_or("state not initialized")?;
    drop(s);
    Ok(designer_state)
}

/// Creates a new puzzle whose solution is a random Latin square.
///
/// `solution` holds the fixed Latin-square values (a single value per cell).
/// `current` starts as the same Latin-square grid, constrained by the (empty) puzzle.
///
/// # Errors
/// Returns an error string if `n` is invalid or the state lock is poisoned.
#[tauri::command]
pub fn new_latin_square(n: usize, state: TauriState<Mutex<AppState>>) -> Result<State, String> {
    let puzzle = Puzzle::new(n).map_err(|e| e.to_string())?;
    let latin = generate_latin_square(n, &mut rand::rng());
    let solution = Grid::from_latin_square(n, &latin).map_err(|e| e.to_string())?;
    let current = solution.clone();
    let mut s = state.lock().map_err(|e| e.to_string())?;
    s.puzzle = Some(puzzle);
    s.solution = Some(solution);
    s.current = Some(current);
    s.path = None;
    s.dirty = true;
    let designer_state = s.to_designer_state().ok_or("state not initialized")?;
    drop(s);
    Ok(designer_state)
}

/// # Errors
/// Returns an error string if no puzzle is loaded, serialization fails, or the file cannot be
/// written.
#[tauri::command]
pub fn save_puzzle<R: Runtime>(
    path: String,
    app: AppHandle<R>,
    state: TauriState<Mutex<AppState>>,
) -> Result<SaveResult, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let puzzle = s.puzzle.as_ref().ok_or("no puzzle loaded")?.clone();
    let solution = s.solution.clone();
    let envelope = SaveEnvelope {
        version: SAVE_VERSION,
        puzzle,
        solution,
    };
    let json = to_string_pretty(&envelope).map_err(|e| e.to_string())?;
    fs::write(&path, &json).map_err(|e| e.to_string())?;
    s.path = Some(path.clone());
    s.dirty = false;
    let active = s.active;
    drop(s);
    write_recent(&app, Some(&path), active);
    Ok(SaveResult { path })
}

/// # Errors
/// Returns an error string if the file cannot be read, JSON is malformed, or the version is
/// unsupported.
#[tauri::command]
pub fn load_puzzle<R: Runtime>(
    path: String,
    app: AppHandle<R>,
    state: TauriState<Mutex<AppState>>,
) -> Result<State, String> {
    let json = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let envelope: SaveEnvelope = from_str(&json).map_err(|e| e.to_string())?;
    if envelope.version != SAVE_VERSION {
        return Err(format!("unsupported version: {}", envelope.version));
    }
    let puzzle = envelope.puzzle;
    let solution = envelope.solution;
    let current = constrain_current(solution.as_ref(), &puzzle).map_err(|e| e.to_string())?;
    let mut s = state.lock().map_err(|e| e.to_string())?;
    s.puzzle = Some(puzzle);
    s.solution = solution;
    s.current = Some(current);
    s.path = Some(path.clone());
    s.dirty = false;
    s.active = None;
    let designer_state = s.to_designer_state().ok_or("state not initialized")?;
    drop(s);
    write_recent(&app, Some(&path), None);
    Ok(designer_state)
}

/// Returns the document state (dirty flag and current file path).
#[tauri::command]
pub fn get_doc_state(state: TauriState<Mutex<AppState>>) -> DocState {
    let s = state.lock().unwrap_or_else(PoisonError::into_inner);
    DocState {
        dirty: s.dirty,
        path: s.path.clone(),
    }
}

/// Returns the current designer [`State`], or `None` if no puzzle is loaded.
///
/// Called at startup so the frontend can restore the last session.
#[tauri::command]
pub fn get_puzzle(state: TauriState<Mutex<AppState>>) -> Option<State> {
    let s = state.lock().ok()?;
    s.to_designer_state()
}

/// Persists the active cell position.
///
/// # Errors
/// Returns an error string if the state lock is poisoned.
#[tauri::command]
pub fn set_active_cell<R: Runtime>(
    active: Cell,
    app: AppHandle<R>,
    state: TauriState<Mutex<AppState>>,
) -> Result<(), String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    s.active = Some(active);
    let path = s.path.clone();
    drop(s);
    write_recent(&app, path.as_deref(), Some(active));
    Ok(())
}

/// Exits the application immediately.
#[tauri::command]
pub fn quit_app<R: Runtime>(app: AppHandle<R>) {
    app.exit(0);
}

/// # Errors
/// Returns an error string if no window is found or the title cannot be set.
#[tauri::command]
pub fn set_window_title<R: Runtime>(title: String, app: AppHandle<R>) -> Result<(), String> {
    app.get_webview_window("main")
        .ok_or_else(|| "no main window".to_string())?
        .set_title(&title)
        .map_err(|e| e.to_string())
}

/// Adds a cage to the current puzzle for the given cells and operator.
///
/// In Without-Solution mode the caller supplies `target` directly. In
/// With-Solution mode `target` is `None` and the value is computed from the
/// `solution` singleton values:
/// - `Given` and single-cell: the cell's solution value.
/// - `Add`: sum of all solution values.
/// - `Multiply`: product of all solution values.
/// - `Subtract` and `Divide` (2-cell only): difference or ratio of the two solution values.
///
/// Returns the updated designer `State`.
///
/// # Errors
/// Returns an error string if no puzzle is loaded, the cells form an invalid polyomino, or
/// `operator` is not valid for the polyomino size.
#[tauri::command]
pub fn add_region(
    cells: Vec<Cell>,
    operator: Operator,
    target: Option<u64>,
    state: TauriState<Mutex<AppState>>,
) -> Result<State, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let puzzle = s.puzzle.as_ref().ok_or("no puzzle loaded")?;
    let poly = Polyomino::from_cells(&cells).map_err(|e| e.to_string())?;

    let target = match target {
        // Without-Solution mode: the author chose the target.
        Some(t) => t,
        // With-Solution mode: derive the target from the fixed solution.
        None => {
            let true_values: Option<Vec<u64>> = s.solution.as_ref().and_then(|grid| {
                cells
                    .iter()
                    .map(|&cell| {
                        let v = grid.cell_values(cell).ok()?;
                        v.is_singleton()
                            .then(|| v.values().first().copied().map(u64::from))?
                    })
                    .collect()
            });
            match (&operator, true_values.as_deref()) {
                (Operator::Given, Some(vals)) => vals[0],
                (Operator::Add, Some(vals)) => vals.iter().sum(),
                (Operator::Multiply, Some(vals)) => vals.iter().product(),
                (Operator::Subtract, Some(vals)) => vals[0].abs_diff(vals[1]),
                (Operator::Divide, Some(vals)) => vals[0].max(vals[1]) / vals[0].min(vals[1]),
                _ => 0,
            }
        }
    };
    let operation = Operation::new(operator, target);

    let cage = Cage::new(poly, operation);
    let new_puzzle = puzzle.insert_cage(cage).map_err(|e| e.to_string())?;
    let new_current =
        constrain_current(s.solution.as_ref(), &new_puzzle).map_err(|e| e.to_string())?;
    s.puzzle = Some(new_puzzle);
    s.current = Some(new_current);
    s.dirty = true;
    let designer_state = s.to_designer_state().ok_or("state not initialized")?;
    drop(s);
    Ok(designer_state)
}

/// Switches the current puzzle from Without-Solution to With-Solution by
/// snapshotting its unique completion into `solution`.
///
/// # Errors
/// Returns an error string if no puzzle is loaded or the puzzle does not have
/// exactly one global completion.
#[tauri::command]
pub fn fix(state: TauriState<Mutex<AppState>>) -> Result<State, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let mut designer = s.to_designer_state().ok_or("no puzzle loaded")?;
    designer.fix().map_err(|e| e.to_string())?;
    s.solution.clone_from(&designer.solution);
    s.dirty = true;
    drop(s);
    Ok(designer)
}

/// Switches the current puzzle from With-Solution to Without-Solution by
/// discarding its `solution`.
///
/// # Errors
/// Returns an error string if no puzzle is loaded.
#[tauri::command]
pub fn unfix(state: TauriState<Mutex<AppState>>) -> Result<State, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let mut designer = s.to_designer_state().ok_or("no puzzle loaded")?;
    designer.unfix();
    s.solution = None;
    s.dirty = true;
    drop(s);
    Ok(designer)
}

/// Removes the cage whose cell set matches `cells` from the current puzzle.
///
/// Returns the updated designer `State`.
///
/// # Errors
/// Returns an error string if no puzzle is loaded, the cells form an invalid
/// polyomino, or no matching cage is found.
#[tauri::command]
pub fn remove_region(
    cells: Vec<Cell>,
    state: TauriState<Mutex<AppState>>,
) -> Result<State, String> {
    let mut s = state.lock().map_err(|e| e.to_string())?;
    let puzzle = s.puzzle.as_ref().ok_or("no puzzle loaded")?;
    let target_cells: HashSet<_> = cells.iter().copied().collect();
    let n = puzzle.n();
    let remaining_cages: Vec<Cage> = puzzle
        .cages()
        .filter(|cage| {
            let cage_cells: HashSet<_> = cage.cells().into_iter().collect();
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
    let new_current =
        constrain_current(s.solution.as_ref(), &new_puzzle).map_err(|e| e.to_string())?;
    s.puzzle = Some(new_puzzle);
    s.current = Some(new_current);
    s.dirty = true;
    let designer_state = s.to_designer_state().ok_or("state not initialized")?;
    drop(s);
    Ok(designer_state)
}
