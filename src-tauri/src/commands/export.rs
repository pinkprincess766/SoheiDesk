use crate::db::DbState;
use crate::error::AppResult;
use crate::export::{self, ExportTemplate, ExportTemplateInput, MultiExportPreview};
use tauri::State;

#[tauri::command]
pub fn list_export_templates(db: State<'_, DbState>) -> AppResult<Vec<ExportTemplate>> {
    export::list_export_templates(&db)
}

#[tauri::command]
pub fn create_export_template(
    db: State<'_, DbState>,
    input: ExportTemplateInput,
) -> AppResult<ExportTemplate> {
    export::create_export_template(&db, input)
}

#[tauri::command]
pub fn delete_export_template(db: State<'_, DbState>, id: String) -> AppResult<()> {
    export::delete_export_template(&db, &id)
}

#[tauri::command]
pub fn preview_entry_export(
    db: State<'_, DbState>,
    entry_id: String,
    format: String,
    template_id: Option<String>,
    author: Option<String>,
    project: Option<String>,
) -> AppResult<MultiExportPreview> {
    export::preview_entry_export(&db, &entry_id, &format, template_id, author, project)
}

#[tauri::command]
pub fn export_entry_formatted(
    db: State<'_, DbState>,
    entry_id: String,
    format: String,
    path: String,
    template_id: Option<String>,
    author: Option<String>,
    project: Option<String>,
) -> AppResult<()> {
    export::export_entry_to_path(&db, &entry_id, &format, &path, template_id, author, project)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn preview_period_export(
    db: State<'_, DbState>,
    from_date: String,
    to_date: String,
    format: String,
    template_id: Option<String>,
    title: Option<String>,
    author: Option<String>,
    project: Option<String>,
    tag_filter: Option<String>,
) -> AppResult<MultiExportPreview> {
    export::preview_period_export(
        &db,
        &from_date,
        &to_date,
        &format,
        template_id,
        title,
        author,
        project,
        tag_filter,
    )
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn export_period_formatted(
    db: State<'_, DbState>,
    from_date: String,
    to_date: String,
    format: String,
    path: String,
    template_id: Option<String>,
    title: Option<String>,
    author: Option<String>,
    project: Option<String>,
    tag_filter: Option<String>,
) -> AppResult<()> {
    export::export_period_to_path(
        &db,
        &from_date,
        &to_date,
        &format,
        &path,
        template_id,
        title,
        author,
        project,
        tag_filter,
    )
}
