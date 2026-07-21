use crate::db::DbState;
use crate::error::AppResult;
use crate::integrations::{self, ChromaRange};
use tauri::State;

#[tauri::command]
pub fn open_in_chroma(
    db: State<'_, DbState>,
    spectrum_path: String,
    range: Option<ChromaRange>,
) -> AppResult<String> {
    integrations::open_in_chroma(&db, spectrum_path, range)
}
