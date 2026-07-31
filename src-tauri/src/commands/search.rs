use crate::db::DbState;
use crate::error::AppResult;
use crate::search::{self, SearchHit, SearchState};
use tauri::State;

#[tauri::command]
pub fn search_all(
    search: State<'_, SearchState>,
    query: String,
    limit: Option<usize>,
) -> AppResult<Vec<SearchHit>> {
    search.search(&query, limit.unwrap_or(30))
}

#[tauri::command]
pub fn reindex_all(db: State<'_, DbState>, search: State<'_, SearchState>) -> AppResult<u64> {
    search.reindex_all(&db)
}

#[tauri::command]
pub fn index_document(
    search: State<'_, SearchState>,
    id: String,
    title: String,
    path: String,
    doc_type: String,
    text: Option<String>,
) -> AppResult<()> {
    search::index_opened_document(&search, &id, &title, &path, &doc_type, text.as_deref())
}
