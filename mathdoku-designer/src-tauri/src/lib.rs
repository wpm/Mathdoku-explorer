#[cfg(any())]
mod old;

pub mod commands;

use std::sync::Mutex;

use mathdoku::Puzzle;
use tauri::image::Image;
use tauri::menu::{AboutMetadata, Menu, MenuItemBuilder, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Runtime, WindowEvent};

use commands::{read_recent, AppState, SaveEnvelope, SAVE_VERSION};

const EVENT_NEW: &str = "menu-new";
const EVENT_OPEN: &str = "menu-open";
const EVENT_SAVE: &str = "menu-save";
const EVENT_SAVE_AS: &str = "menu-save-as";
const EVENT_REQUEST_CLOSE: &str = "request-close";

// ---- menu ----

fn build_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let new = MenuItemBuilder::with_id("new", "New…")
        .accelerator("CmdOrCtrl+N")
        .build(app)?;
    let open = MenuItemBuilder::with_id("open", "Open…")
        .accelerator("CmdOrCtrl+O")
        .build(app)?;
    let save = MenuItemBuilder::with_id("save", "Save")
        .accelerator("CmdOrCtrl+S")
        .build(app)?;
    let save_as = MenuItemBuilder::with_id("save_as", "Save As…")
        .accelerator("CmdOrCtrl+Shift+S")
        .build(app)?;
    let file_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &new,
            &PredefinedMenuItem::separator(app)?,
            &open,
            &save,
            &save_as,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;
    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;

    #[cfg(target_os = "macos")]
    {
        let app_menu = Submenu::with_items(
            app,
            "Mathdoku Designer",
            true,
            &[
                &PredefinedMenuItem::about(
                    app,
                    Some("About Mathdoku Designer"),
                    Some(AboutMetadata {
                        icon: Some(Image::from_bytes(include_bytes!("../icons/128x128.png"))?),
                        ..Default::default()
                    }),
                )?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::services(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::hide(app, None)?,
                &PredefinedMenuItem::hide_others(app, None)?,
                &PredefinedMenuItem::separator(app)?,
                &PredefinedMenuItem::quit(app, None)?,
            ],
        )?;
        let view_menu = Submenu::with_items(
            app,
            "View",
            true,
            &[&PredefinedMenuItem::fullscreen(app, None)?],
        )?;
        Menu::with_items(
            app,
            &[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu],
        )
    }

    #[cfg(not(target_os = "macos"))]
    Menu::with_items(app, &[&file_menu, &edit_menu, &window_menu])
}

#[allow(clippy::needless_pass_by_value)]
fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, event: tauri::menu::MenuEvent) {
    let event_name = match event.id().as_ref() {
        "new" => EVENT_NEW,
        "open" => EVENT_OPEN,
        "save" => EVENT_SAVE,
        "save_as" => EVENT_SAVE_AS,
        _ => return,
    };
    let _ = app.emit(event_name, ());
}

fn handle_window_event<R: Runtime>(window: &tauri::Window<R>, event: &WindowEvent) {
    let WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    let app = window.app_handle();
    let dirty = app
        .try_state::<Mutex<AppState>>()
        .and_then(|s| s.lock().ok().map(|s| s.dirty))
        .unwrap_or(false);
    if dirty {
        api.prevent_close();
        let _ = app.emit(EVENT_REQUEST_CLOSE, ());
    }
}

// ---- startup ----

fn try_restore<R: Runtime>(app: &AppHandle<R>) -> Option<Puzzle> {
    let record = read_recent(app)?;
    let path = record.path?;
    let json = std::fs::read_to_string(&path).ok()?;
    let envelope: SaveEnvelope = serde_json::from_str(&json).ok()?;
    if envelope.version != SAVE_VERSION {
        return None;
    }
    let state = app.try_state::<Mutex<AppState>>()?;
    let mut s = state.lock().ok()?;
    s.puzzle = Some(envelope.puzzle.clone());
    s.path = Some(path);
    s.dirty = false;
    s.view_state = record.view.unwrap_or_default();
    drop(s);
    Some(envelope.puzzle)
}

// ---- run ----

/// # Panics
/// Panics if the Tauri application fails to start.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[allow(clippy::expect_used)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(AppState::default()))
        .menu(build_menu)
        .on_menu_event(handle_menu_event)
        .on_window_event(handle_window_event)
        .setup(|app| {
            if try_restore(app.handle()).is_none() {
                if let Ok(mut s) = app.state::<Mutex<AppState>>().lock() {
                    s.puzzle = Some(Puzzle::new(9).expect("9 is a valid puzzle size"));
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::new_puzzle,
            commands::generate_puzzle,
            commands::save_puzzle,
            commands::load_puzzle,
            commands::get_doc_state,
            commands::get_puzzle,
            commands::get_view_state,
            commands::set_view_state,
            commands::set_window_title,
            commands::quit_app,
            commands::add_region,
            commands::remove_region,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use commands::{
        get_doc_state, get_puzzle, load_puzzle, new_puzzle, recent_path, save_puzzle, SaveEnvelope,
        SAVE_VERSION,
    };

    // Serialize tests that read/write the shared on-disk recent file.
    static RECENT_FILE_LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
    fn recent_file_lock() -> std::sync::MutexGuard<'static, ()> {
        RECENT_FILE_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap()
    }

    #[derive(serde::Serialize)]
    struct RecentRecord {
        path: Option<String>,
    }

    fn write_recent_test(recent: &std::path::Path, puzzle_path: &str) {
        if let Some(parent) = recent.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string(&RecentRecord {
            path: Some(puzzle_path.to_owned()),
        })
        .unwrap();
        std::fs::write(recent, json).unwrap();
    }

    fn mock_app() -> tauri::App<tauri::test::MockRuntime> {
        tauri::test::mock_app()
    }

    fn app_with_state() -> tauri::App<tauri::test::MockRuntime> {
        let app = mock_app();
        app.manage(Mutex::new(AppState::default()));
        app
    }

    fn app_with_puzzle(n: usize) -> tauri::App<tauri::test::MockRuntime> {
        let app = mock_app();
        app.manage(Mutex::new(AppState {
            puzzle: Some(Puzzle::new(n).unwrap()),
            ..AppState::default()
        }));
        app
    }

    // ---- new_puzzle ----

    #[test]
    fn new_puzzle_sets_puzzle_and_dirty() {
        let app = app_with_state();
        let result = new_puzzle(4, app.state::<Mutex<AppState>>()).unwrap();
        assert_eq!(result.n(), 4);
        let binding = app.state::<Mutex<AppState>>();
        let s = binding.lock().unwrap();
        assert!(s.puzzle.is_some());
        assert!(s.dirty);
        assert!(s.path.is_none());
        drop(s);
    }

    #[test]
    fn new_puzzle_clears_existing_path() {
        let app = mock_app();
        app.manage(Mutex::new(AppState {
            path: Some("/old/path.mathdoku".to_string()),
            ..AppState::default()
        }));
        new_puzzle(4, app.state::<Mutex<AppState>>()).unwrap();
        let binding = app.state::<Mutex<AppState>>();
        let s = binding.lock().unwrap();
        assert!(s.path.is_none());
        drop(s);
    }

    #[test]
    fn new_puzzle_rejects_invalid_size() {
        let app = app_with_state();
        assert!(new_puzzle(0, app.state::<Mutex<AppState>>()).is_err());
    }

    // ---- save_puzzle / load_puzzle round-trip ----

    #[test]
    fn save_and_load_round_trips_puzzle() {
        let _guard = recent_file_lock();
        let app = app_with_puzzle(5);
        let path = std::env::temp_dir()
            .join("mathdoku_test_save_load.mathdoku")
            .to_str()
            .unwrap()
            .to_string();

        let result = save_puzzle(
            path.clone(),
            app.handle().clone(),
            app.state::<Mutex<AppState>>(),
        )
        .unwrap();
        assert_eq!(result.path, path);

        // Load into a fresh app state.
        let app2 = app_with_state();
        let puzzle = load_puzzle(
            path.clone(),
            app2.handle().clone(),
            app2.state::<Mutex<AppState>>(),
        )
        .unwrap();
        assert_eq!(puzzle.n(), 5);
        let binding = app2.state::<Mutex<AppState>>();
        let s = binding.lock().unwrap();
        assert!(!s.dirty);
        assert_eq!(s.path.as_deref(), Some(path.as_str()));
        drop(s);
    }

    #[test]
    fn save_puzzle_sets_path_and_clears_dirty() {
        let _guard = recent_file_lock();
        let app = app_with_puzzle(4);
        // Mark dirty first.
        app.state::<Mutex<AppState>>().lock().unwrap().dirty = true;
        let path = std::env::temp_dir()
            .join("mathdoku_test_save_dirty.mathdoku")
            .to_str()
            .unwrap()
            .to_string();

        save_puzzle(
            path.clone(),
            app.handle().clone(),
            app.state::<Mutex<AppState>>(),
        )
        .unwrap();
        let binding = app.state::<Mutex<AppState>>();
        let s = binding.lock().unwrap();
        assert!(!s.dirty);
        assert_eq!(s.path.as_deref(), Some(path.as_str()));
        drop(s);
    }

    #[test]
    fn save_puzzle_errors_when_no_puzzle_loaded() {
        let app = app_with_state();
        let path = "/tmp/mathdoku_no_puzzle.mathdoku".to_string();
        assert!(save_puzzle(path, app.handle().clone(), app.state::<Mutex<AppState>>()).is_err());
    }

    #[test]
    fn save_puzzle_errors_on_bad_path() {
        let app = app_with_puzzle(4);
        let bad_path = "/nonexistent/dir/puzzle.mathdoku".to_string();
        assert!(save_puzzle(
            bad_path,
            app.handle().clone(),
            app.state::<Mutex<AppState>>()
        )
        .is_err());
    }

    #[test]
    fn load_puzzle_errors_on_missing_file() {
        let app = app_with_state();
        assert!(load_puzzle(
            "/no/such/file.mathdoku".to_string(),
            app.handle().clone(),
            app.state::<Mutex<AppState>>(),
        )
        .is_err());
    }

    #[test]
    fn load_puzzle_rejects_wrong_version() {
        let path = std::env::temp_dir()
            .join("mathdoku_test_bad_version.mathdoku")
            .to_str()
            .unwrap()
            .to_string();
        let puzzle = Puzzle::new(3).unwrap();
        let bad = serde_json::json!({ "version": 99, "puzzle": puzzle });
        std::fs::write(&path, serde_json::to_string(&bad).unwrap()).unwrap();

        let app = app_with_state();
        let err =
            load_puzzle(path, app.handle().clone(), app.state::<Mutex<AppState>>()).unwrap_err();
        assert!(err.contains("unsupported version"));
    }

    #[test]
    fn load_puzzle_rejects_malformed_json() {
        let path = std::env::temp_dir()
            .join("mathdoku_test_malformed.mathdoku")
            .to_str()
            .unwrap()
            .to_string();
        std::fs::write(&path, "not json").unwrap();

        let app = app_with_state();
        assert!(load_puzzle(path, app.handle().clone(), app.state::<Mutex<AppState>>()).is_err());
    }

    // ---- get_doc_state ----

    #[test]
    fn get_doc_state_returns_current_values() {
        let app = mock_app();
        app.manage(Mutex::new(AppState {
            dirty: true,
            path: Some("/some/path.mathdoku".to_string()),
            ..AppState::default()
        }));

        let doc = get_doc_state(app.state::<Mutex<AppState>>());
        assert!(doc.dirty);
        assert_eq!(doc.path.as_deref(), Some("/some/path.mathdoku"));
    }

    #[test]
    fn get_doc_state_default_is_clean_with_no_path() {
        let app = app_with_state();
        let doc = get_doc_state(app.state::<Mutex<AppState>>());
        assert!(!doc.dirty);
        assert!(doc.path.is_none());
    }

    // ---- get_puzzle ----

    #[test]
    fn get_puzzle_returns_none_when_no_puzzle() {
        let app = app_with_state();
        assert!(get_puzzle(app.state::<Mutex<AppState>>()).is_none());
    }

    #[test]
    fn get_puzzle_returns_puzzle_when_loaded() {
        let app = app_with_puzzle(6);
        let p = get_puzzle(app.state::<Mutex<AppState>>()).unwrap();
        assert_eq!(p.n(), 6);
    }

    // ---- try_restore ----

    #[test]
    fn try_restore_returns_none_when_no_recent_file() {
        let _guard = recent_file_lock();
        let app = app_with_state();
        if let Some(recent) = recent_path(app.handle()) {
            let _ = std::fs::remove_file(recent);
        }
        assert!(try_restore(app.handle()).is_none());
    }

    #[test]
    fn try_restore_loads_puzzle_from_saved_file() {
        let _guard = recent_file_lock();
        let puzzle_path = std::env::temp_dir()
            .join("mathdoku_test_restore.mathdoku")
            .to_str()
            .unwrap()
            .to_string();
        let puzzle = Puzzle::new(4).unwrap();
        let envelope = SaveEnvelope {
            version: SAVE_VERSION,
            puzzle,
        };
        std::fs::write(
            &puzzle_path,
            serde_json::to_string_pretty(&envelope).unwrap(),
        )
        .unwrap();

        let app = app_with_state();
        let recent = recent_path(app.handle()).unwrap();
        write_recent_test(&recent, &puzzle_path);

        let restored = try_restore(app.handle());
        assert!(restored.is_some());
        assert_eq!(restored.unwrap().n(), 4);
        let binding = app.state::<Mutex<AppState>>();
        let s = binding.lock().unwrap();
        assert!(!s.dirty);
        assert_eq!(s.path.as_deref(), Some(puzzle_path.as_str()));
        drop(s);

        let _ = std::fs::remove_file(&recent);
    }

    #[test]
    fn try_restore_returns_none_for_wrong_version() {
        let _guard = recent_file_lock();
        let puzzle_path = std::env::temp_dir()
            .join("mathdoku_test_restore_bad_ver.mathdoku")
            .to_str()
            .unwrap()
            .to_string();
        let puzzle = Puzzle::new(3).unwrap();
        let bad = serde_json::json!({ "version": 99, "puzzle": puzzle });
        std::fs::write(&puzzle_path, serde_json::to_string(&bad).unwrap()).unwrap();

        let app = app_with_state();
        let recent = recent_path(app.handle()).unwrap();
        write_recent_test(&recent, &puzzle_path);

        assert!(try_restore(app.handle()).is_none());
        let _ = std::fs::remove_file(&recent);
    }

    // ---- handle_menu_event ----

    #[test]
    fn handle_menu_event_unknown_id_does_not_panic() {
        let app = mock_app();
        handle_menu_event(
            app.handle(),
            tauri::menu::MenuEvent {
                id: tauri::menu::MenuId::new("unknown"),
            },
        );
    }

    #[test]
    fn handle_menu_event_known_ids_emit_without_panic() {
        let app = mock_app();
        for id in ["new", "open", "save", "save_as"] {
            handle_menu_event(
                app.handle(),
                tauri::menu::MenuEvent {
                    id: tauri::menu::MenuId::new(id),
                },
            );
        }
    }
}
