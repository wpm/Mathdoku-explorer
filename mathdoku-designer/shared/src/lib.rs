//! Types shared between the Tauri backend (`src-tauri`) and the Leptos
//! frontend (`src`). Keeping them here avoids duplicating serde definitions
//! and ensures both sides agree on a serialization format over the IPC bridge.

/// Which interaction mode the puzzle editor is in.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Cell,
    Slot,
}

/// Persisted editor view state: which mode is active and where the selection is.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ViewState {
    pub mode: Mode,
    pub cell_row: usize,
    pub cell_col: usize,
    pub slot_idx: usize,
}

/// Document state returned by `get_doc_state`.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DocState {
    pub dirty: bool,
    pub path: Option<String>,
}
