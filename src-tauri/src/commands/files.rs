use crate::db::{with_conn, DbState};
use crate::error::{AppError, AppResult};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

#[derive(Serialize)]
pub struct FileBytes {
    pub path: String,
    pub base64: String,
    pub mime: String,
    pub size: u64,
}

fn normalize_path(path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    // Prefer canonical path when file exists
    std::fs::canonicalize(&p).unwrap_or(p)
}

fn strip_private_prefix(s: &str) -> String {
    // macOS may expose /private/var vs /var
    s.replace("/private/var", "/var")
        .replace("/private/tmp", "/tmp")
}

fn paths_match(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let na = normalize_path(a);
    let nb = normalize_path(b);
    if na == nb {
        return true;
    }
    strip_private_prefix(&na.to_string_lossy()) == strip_private_prefix(&nb.to_string_lossy())
}

/// True if `path` is inside app_data/media/ (PDF cache + DOCX images we wrote).
fn is_under_media_cache(db: &DbState, path: &str) -> bool {
    let media = db.data_dir.join("media");
    let media_canon = std::fs::canonicalize(&media).unwrap_or_else(|_| media.clone());
    let target = normalize_path(path);
    let media_s = strip_private_prefix(&media_canon.to_string_lossy());
    let target_s = strip_private_prefix(&target.to_string_lossy());
    // Ensure directory boundary (media/foo, not mediaevil)
    target_s == media_s
        || target_s.starts_with(&format!("{media_s}/"))
        || target_s.starts_with(&format!("{media_s}\\"))
}

fn is_path_authorized(db: &DbState, path: &str) -> AppResult<bool> {
    // App-local cache: only files we created from dialog-opened documents
    if is_under_media_cache(db, path) {
        return Ok(true);
    }

    with_conn(db, |conn| {
        let mut stmt = conn.prepare("SELECT last_path FROM documents WHERE last_path IS NOT NULL")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for r in rows {
            let stored = r?;
            if paths_match(&stored, path) {
                return Ok(true);
            }
        }
        Ok(false)
    })
}

fn is_document_authorized(db: &DbState, document_id: &str, path: &str) -> AppResult<bool> {
    // Cache under media/ is always tied to content opened via the app
    if is_under_media_cache(db, path) {
        // Still require the document id to exist (prevents free-form id probing of arbitrary FS —
        // media paths are already constrained above).
        let exists: bool = with_conn(db, |conn| {
            let n: i64 = conn.query_row(
                "SELECT COUNT(1) FROM documents WHERE id = ?1",
                [document_id],
                |row| row.get(0),
            )?;
            Ok(n > 0)
        })?;
        return Ok(exists);
    }

    with_conn(db, |conn| {
        let last: Option<String> = conn
            .query_row(
                "SELECT last_path FROM documents WHERE id = ?1",
                [document_id],
                |row| row.get(0),
            )
            .ok()
            .flatten();
        if let Some(stored) = last {
            return Ok(paths_match(&stored, path));
        }
        Ok(false)
    })
}

/// Read file bytes for PDF viewer etc.
/// Only for: (1) paths registered via Open dialog, or (2) app media cache.
#[tauri::command]
pub fn read_authorized_file(
    db: State<'_, DbState>,
    path: String,
    document_id: Option<String>,
) -> AppResult<FileBytes> {
    // Reject path traversal / empty
    if path.trim().is_empty() || path.contains('\0') {
        return Err(AppError::Message("invalid path".into()));
    }

    let authorized = if let Some(ref id) = document_id {
        is_document_authorized(&db, id, &path)? || is_path_authorized(&db, &path)?
    } else {
        is_path_authorized(&db, &path)?
    };

    if !authorized {
        return Err(AppError::Message(format!(
            "path is not authorized: {path}. Re-open the file via Open dialog."
        )));
    }

    let path_buf = PathBuf::from(&path);
    if !path_buf.is_file() {
        let canon = normalize_path(&path);
        if !canon.is_file() {
            return Err(AppError::Message(format!("file not found: {path}")));
        }
        // Re-check authorization on canonical path (symlink safety)
        let canon_s = canon.to_string_lossy().to_string();
        let still_ok = if let Some(ref id) = document_id {
            is_document_authorized(&db, id, &canon_s)? || is_path_authorized(&db, &canon_s)?
        } else {
            is_path_authorized(&db, &canon_s)?
        };
        if !still_ok {
            return Err(AppError::Message("path is not authorized after resolve".into()));
        }
        return read_path(canon);
    }
    read_path(path_buf)
}

fn read_path(path: PathBuf) -> AppResult<FileBytes> {
    let meta = std::fs::metadata(&path)?;
    let size = meta.len();
    // 200MB hard cap for IPC base64 (large textbooks / scans)
    if size > 200 * 1024 * 1024 {
        return Err(AppError::Message(format!(
            "file too large for in-app load ({:.1} MB). Max 200 MB.",
            size as f64 / (1024.0 * 1024.0)
        )));
    }
    let bytes = std::fs::read(&path)?;
    let lower = path.to_string_lossy().to_ascii_lowercase();
    let mime = if lower.ends_with(".pdf") {
        "application/pdf"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else {
        "application/octet-stream"
    };
    Ok(FileBytes {
        path: path.to_string_lossy().to_string(),
        base64: B64.encode(bytes),
        mime: mime.into(),
        size,
    })
}
