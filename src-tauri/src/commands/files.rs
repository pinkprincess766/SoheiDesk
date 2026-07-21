use crate::db::{with_conn, DbState};
use crate::error::{AppError, AppResult};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
pub struct FileBytes {
    pub path: String,
    pub base64: String,
    pub mime: String,
}

fn is_path_authorized(db: &DbState, path: &str) -> AppResult<bool> {
    with_conn(db, |conn| {
        let mut stmt = conn.prepare("SELECT 1 FROM documents WHERE last_path = ?1 LIMIT 1")?;
        let exists = stmt.exists([path])?;
        Ok(exists)
    })
}

/// Read file bytes only if path is registered in the library (user-selected).
#[tauri::command]
pub fn read_authorized_file(db: State<'_, DbState>, path: String) -> AppResult<FileBytes> {
    if !is_path_authorized(&db, &path)? {
        return Err(AppError::Message(
            "path is not authorized; open the file via dialog first".into(),
        ));
    }
    let bytes = std::fs::read(&path)?;
    let mime = if path.to_ascii_lowercase().ends_with(".pdf") {
        "application/pdf"
    } else {
        "application/octet-stream"
    };
    Ok(FileBytes {
        path,
        base64: B64.encode(bytes),
        mime: mime.into(),
    })
}
