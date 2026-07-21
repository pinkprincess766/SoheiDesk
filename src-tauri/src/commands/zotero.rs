use crate::db::DbState;
use crate::error::AppResult;
use crate::integrations::zotero::{self, ZoteroItem};
use crate::library::DocumentRecord;
use tauri::State;

#[tauri::command]
pub fn zotero_list_items(db_path: String, limit: Option<usize>) -> AppResult<Vec<ZoteroItem>> {
    zotero::list_items(&db_path, limit.unwrap_or(100))
}

#[tauri::command]
pub fn zotero_import_paths(
    db: State<'_, DbState>,
    paths: Vec<String>,
) -> AppResult<Vec<DocumentRecord>> {
    zotero::import_attachments(&db, paths)
}

#[tauri::command]
pub fn zotero_save_db_path(db: State<'_, DbState>, path: String) -> AppResult<()> {
    zotero::save_zotero_path(&db, &path)
}
