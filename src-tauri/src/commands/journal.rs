use crate::db::DbState;
use crate::error::AppResult;
use crate::journal::{self, ExportPreview, JournalEntry, JournalEntryInput};
use crate::search::{self, SearchState};
use crate::templates::{self, TemplateInput, TemplateRecord};
use tauri::State;

#[tauri::command]
pub fn list_templates(db: State<'_, DbState>) -> AppResult<Vec<TemplateRecord>> {
    templates::list_templates(&db)
}

#[tauri::command]
pub fn create_template(db: State<'_, DbState>, input: TemplateInput) -> AppResult<TemplateRecord> {
    templates::create_template(&db, input)
}

#[tauri::command]
pub fn update_template(
    db: State<'_, DbState>,
    id: String,
    input: TemplateInput,
) -> AppResult<TemplateRecord> {
    templates::update_template(&db, &id, input)
}

#[tauri::command]
pub fn delete_template(db: State<'_, DbState>, id: String) -> AppResult<()> {
    templates::delete_template(&db, &id)
}

#[tauri::command]
pub fn list_journal_entries(db: State<'_, DbState>) -> AppResult<Vec<JournalEntry>> {
    journal::list_entries(&db)
}

#[tauri::command]
pub fn get_journal_entry(db: State<'_, DbState>, id: String) -> AppResult<JournalEntry> {
    journal::get_entry(&db, &id)
}

#[tauri::command]
pub fn create_journal_entry(
    db: State<'_, DbState>,
    search: State<'_, SearchState>,
    input: JournalEntryInput,
) -> AppResult<JournalEntry> {
    let entry = journal::create_entry(&db, input)?;
    let _ = search::index_journal_entry(&search, &entry.id, &entry.title, &entry.body_md);
    Ok(entry)
}

#[tauri::command]
pub fn update_journal_entry(
    db: State<'_, DbState>,
    search: State<'_, SearchState>,
    id: String,
    input: JournalEntryInput,
) -> AppResult<JournalEntry> {
    let entry = journal::update_entry(&db, &id, input)?;
    let _ = search::index_journal_entry(&search, &entry.id, &entry.title, &entry.body_md);
    Ok(entry)
}

#[tauri::command]
pub fn delete_journal_entry(
    db: State<'_, DbState>,
    search: State<'_, SearchState>,
    id: String,
) -> AppResult<()> {
    journal::delete_entry(&db, &id)?;
    let _ = search.delete(&id);
    Ok(())
}

#[tauri::command]
pub fn preview_journal_export(db: State<'_, DbState>, id: String) -> AppResult<ExportPreview> {
    journal::preview_export(&db, &id)
}

#[tauri::command]
pub fn export_journal_entry(
    db: State<'_, DbState>,
    id: String,
    path: String,
) -> AppResult<()> {
    journal::export_entry_to_path(&db, &id, &path)
}

#[tauri::command]
pub fn save_entry_as_template(
    db: State<'_, DbState>,
    entry_id: String,
    name: String,
) -> AppResult<TemplateRecord> {
    journal::save_entry_as_template(&db, &entry_id, name)
}

#[tauri::command]
pub fn export_template_file(
    db: State<'_, DbState>,
    id: String,
    path: String,
) -> AppResult<()> {
    templates::export_template_to_path(&db, &id, &path)
}

#[tauri::command]
pub fn import_template_file(db: State<'_, DbState>, path: String) -> AppResult<TemplateRecord> {
    templates::import_template_from_path(&db, &path)
}
