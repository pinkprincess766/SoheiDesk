use crate::backup::{self, BackupInfo, BackupRestoreResult, BackupState};
use crate::db::DbState;
use crate::error::AppResult;
use crate::search::SearchState;
use tauri::State;

#[tauri::command]
pub fn create_backup(
    db: State<'_, DbState>,
    backups: State<'_, BackupState>,
) -> AppResult<BackupInfo> {
    backup::create_manual(&db, &backups)
}

#[tauri::command]
pub fn list_backups(db: State<'_, DbState>) -> AppResult<Vec<BackupInfo>> {
    backup::list_backups(&db.data_dir)
}

#[tauri::command]
pub fn restore_backup(
    db: State<'_, DbState>,
    backups: State<'_, BackupState>,
    search: State<'_, SearchState>,
    backup_id: String,
) -> AppResult<BackupRestoreResult> {
    backup::restore_backup(&db, &backups, &search, &backup_id)
}
