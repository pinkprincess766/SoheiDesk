use crate::collab::{CollabState, CollabStatus};
use crate::db::DbState;
use crate::error::AppResult;
use tauri::State;

#[tauri::command]
pub fn collab_status(collab: State<'_, CollabState>) -> CollabStatus {
    collab.status()
}

#[tauri::command]
pub fn collab_start(
    db: State<'_, DbState>,
    collab: State<'_, CollabState>,
    port: Option<u16>,
) -> AppResult<CollabStatus> {
    collab.start(&db, port.unwrap_or(8765))
}

#[tauri::command]
pub fn collab_stop(collab: State<'_, CollabState>) -> CollabStatus {
    collab.stop();
    collab.status()
}
