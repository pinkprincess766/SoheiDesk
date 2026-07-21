use crate::db::DbState;
use crate::error::AppResult;
use crate::literature::{self, BiblioItem, LiteratureHit};
use tauri::State;

#[tauri::command]
pub fn resolve_doi(doi: String) -> AppResult<LiteratureHit> {
    literature::resolve_doi(&doi)
}

#[tauri::command]
pub fn search_arxiv(query: String, max_results: Option<usize>) -> AppResult<Vec<LiteratureHit>> {
    literature::search_arxiv(&query, max_results.unwrap_or(10))
}

#[tauri::command]
pub fn search_pubmed(query: String, max_results: Option<usize>) -> AppResult<Vec<LiteratureHit>> {
    literature::search_pubmed(&query, max_results.unwrap_or(10))
}

#[tauri::command]
pub fn save_literature_hit(db: State<'_, DbState>, hit: LiteratureHit) -> AppResult<BiblioItem> {
    literature::save_hit(&db, &hit)
}

#[tauri::command]
pub fn list_bibliography(db: State<'_, DbState>) -> AppResult<Vec<BiblioItem>> {
    literature::list_biblio(&db)
}

#[tauri::command]
pub fn delete_bibliography_item(db: State<'_, DbState>, id: String) -> AppResult<()> {
    literature::delete_biblio(&db, &id)
}

#[tauri::command]
pub fn export_bibliography(db: State<'_, DbState>, style: String) -> AppResult<String> {
    literature::export_bibliography(&db, &style)
}

#[tauri::command]
pub fn export_bibliography_to_file(
    db: State<'_, DbState>,
    style: String,
    path: String,
) -> AppResult<()> {
    let content = literature::export_bibliography(&db, &style)?;
    std::fs::write(path, content)?;
    Ok(())
}
