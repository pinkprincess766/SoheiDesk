use crate::annotations::{self, Annotation, AnnotationInput};
use crate::db::DbState;
use crate::error::AppResult;
use tauri::State;

#[tauri::command]
pub fn list_annotations(db: State<'_, DbState>, document_id: String) -> AppResult<Vec<Annotation>> {
    annotations::list_for_document(&db, &document_id)
}

#[tauri::command]
pub fn create_annotation(db: State<'_, DbState>, input: AnnotationInput) -> AppResult<Annotation> {
    annotations::create(&db, input)
}

#[tauri::command]
pub fn update_annotation(
    db: State<'_, DbState>,
    id: String,
    content: Option<String>,
    color: Option<String>,
) -> AppResult<Annotation> {
    annotations::update(&db, &id, content, color)
}

#[tauri::command]
pub fn delete_annotation(db: State<'_, DbState>, id: String) -> AppResult<()> {
    annotations::delete(&db, &id)
}

#[tauri::command]
pub fn export_annotations_markdown(
    db: State<'_, DbState>,
    document_id: String,
    doc_title: String,
) -> AppResult<String> {
    annotations::export_markdown(&db, &document_id, &doc_title)
}

#[tauri::command]
pub fn export_annotations_to_file(
    db: State<'_, DbState>,
    document_id: String,
    doc_title: String,
    path: String,
) -> AppResult<()> {
    annotations::export_markdown_to_path(&db, &document_id, &doc_title, &path)
}
