use crate::db::DbState;
use crate::error::AppResult;
use crate::library::{self, DocumentRecord, DocumentVersion, OpenResult};
use tauri::State;

#[tauri::command]
pub fn open_document_path(db: State<'_, DbState>, path: String) -> AppResult<OpenResult> {
    library::open_and_register(&db, std::path::Path::new(&path))
}

#[tauri::command]
pub fn list_documents(db: State<'_, DbState>) -> AppResult<Vec<DocumentRecord>> {
    library::list_documents(&db)
}

#[tauri::command]
pub fn list_document_versions(
    db: State<'_, DbState>,
    document_id: String,
) -> AppResult<Vec<DocumentVersion>> {
    library::list_versions(&db, &document_id)
}

#[tauri::command]
pub fn remove_document(db: State<'_, DbState>, id: String) -> AppResult<()> {
    library::remove_from_library(&db, &id)
}

#[tauri::command]
pub fn reopen_document(db: State<'_, DbState>, id: String) -> AppResult<OpenResult> {
    library::reopen_by_id(&db, &id)
}
