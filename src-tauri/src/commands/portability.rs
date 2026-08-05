use crate::backup::BackupState;
use crate::db::DbState;
use crate::error::AppResult;
use crate::portability::{
    self, PortabilityState, WorkspaceExportResult, WorkspaceImportResult, WorkspacePreview,
};
use crate::search::SearchState;
use tauri::State;

#[tauri::command]
pub fn export_workspace(
    db: State<'_, DbState>,
    portability: State<'_, PortabilityState>,
    path: String,
) -> AppResult<WorkspaceExportResult> {
    portability::export_workspace(&db, &portability, std::path::Path::new(&path))
}

#[tauri::command]
pub fn preview_workspace_import(
    db: State<'_, DbState>,
    portability: State<'_, PortabilityState>,
    path: String,
) -> AppResult<WorkspacePreview> {
    portability::preview_import(&db, &portability, std::path::Path::new(&path))
}

#[tauri::command]
pub fn import_workspace(
    db: State<'_, DbState>,
    backups: State<'_, BackupState>,
    portability: State<'_, PortabilityState>,
    search: State<'_, SearchState>,
    token: String,
    replace_existing: bool,
) -> AppResult<WorkspaceImportResult> {
    portability::import_workspace(
        &db,
        &backups,
        &portability,
        &search,
        &token,
        replace_existing,
    )
}
