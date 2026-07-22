use crate::db::{with_conn, DbState};
use crate::documents::{self, DocType};
use crate::error::{AppError, AppResult};
use crate::parsers;
use chrono::Utc;
use rusqlite::params;
use serde::Serialize;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Serialize, Clone)]
pub struct DocumentRecord {
    pub id: String,
    pub content_hash: String,
    pub title: Option<String>,
    pub last_path: Option<String>,
    pub doc_type: String,
    pub file_size: Option<i64>,
    pub added_at: String,
    pub last_opened_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OpenResult {
    pub document: DocumentRecord,
    pub opened: parsers::OpenedDocument,
}

pub fn list_documents(db: &DbState) -> AppResult<Vec<DocumentRecord>> {
    with_conn(db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, content_hash, title, last_path, doc_type, file_size, added_at, last_opened_at
             FROM documents
             ORDER BY COALESCE(last_opened_at, added_at) DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DocumentRecord {
                id: row.get(0)?,
                content_hash: row.get(1)?,
                title: row.get(2)?,
                last_path: row.get(3)?,
                doc_type: row.get(4)?,
                file_size: row.get(5)?,
                added_at: row.get(6)?,
                last_opened_at: row.get(7)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
}

/// Open a user-selected path: compute content_hash, upsert library row, return payload.
pub fn open_and_register(db: &DbState, path: &Path) -> AppResult<OpenResult> {
    // Pre-hash for cache dir (open_document hashes again — cheap for head/tail hash)
    let (content_hash, _) = documents::content_hash(path)?;
    let cache_dir = db.data_dir.join("media").join(&content_hash);
    let opened = parsers::open_document(path, Some(&cache_dir))?;
    let now = Utc::now().to_rfc3339();
    let doc_type = DocType::from_path(path)?;

    let record = with_conn(db, |conn| {
        let existing: Option<(String, String)> = conn
            .query_row(
                "SELECT id, added_at FROM documents WHERE content_hash = ?1",
                params![&opened.content_hash],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        if let Some((id, added_at)) = existing {
            conn.execute(
                "UPDATE documents SET last_path = ?1, title = COALESCE(title, ?2), last_opened_at = ?3, file_size = ?4, doc_type = ?5
                 WHERE id = ?6",
                params![
                    &opened.path,
                    &opened.title,
                    &now,
                    opened.file_size as i64,
                    doc_type.as_str(),
                    &id
                ],
            )?;
            Ok(DocumentRecord {
                id,
                content_hash: opened.content_hash.clone(),
                title: Some(opened.title.clone()),
                last_path: Some(opened.path.clone()),
                doc_type: doc_type.as_str().to_string(),
                file_size: Some(opened.file_size as i64),
                added_at,
                last_opened_at: Some(now),
            })
        } else {
            let id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO documents (id, content_hash, title, last_path, doc_type, file_size, added_at, last_opened_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    &id,
                    &opened.content_hash,
                    &opened.title,
                    &opened.path,
                    doc_type.as_str(),
                    opened.file_size as i64,
                    &now,
                    &now
                ],
            )?;
            Ok(DocumentRecord {
                id,
                content_hash: opened.content_hash.clone(),
                title: Some(opened.title.clone()),
                last_path: Some(opened.path.clone()),
                doc_type: doc_type.as_str().to_string(),
                file_size: Some(opened.file_size as i64),
                added_at: now.clone(),
                last_opened_at: Some(now),
            })
        }
    })?;

    Ok(OpenResult {
        document: record,
        opened,
    })
}

pub fn remove_from_library(db: &DbState, id: &str) -> AppResult<()> {
    with_conn(db, |conn| {
        let n = conn.execute("DELETE FROM documents WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(AppError::Message("document not found".into()));
        }
        Ok(())
    })
}

pub fn get_document(db: &DbState, id: &str) -> AppResult<DocumentRecord> {
    with_conn(db, |conn| {
        conn.query_row(
            "SELECT id, content_hash, title, last_path, doc_type, file_size, added_at, last_opened_at
             FROM documents WHERE id = ?1",
            params![id],
            |row| {
                Ok(DocumentRecord {
                    id: row.get(0)?,
                    content_hash: row.get(1)?,
                    title: row.get(2)?,
                    last_path: row.get(3)?,
                    doc_type: row.get(4)?,
                    file_size: row.get(5)?,
                    added_at: row.get(6)?,
                    last_opened_at: row.get(7)?,
                })
            },
        )
        .map_err(|_| AppError::Message("document not found".into()))
    })
}

/// Re-open by library id using last_path; re-verify content_hash when possible.
pub fn reopen_by_id(db: &DbState, id: &str) -> AppResult<OpenResult> {
    let doc = get_document(db, id)?;
    let path = doc
        .last_path
        .as_ref()
        .ok_or_else(|| AppError::Message("document has no path; open via dialog".into()))?;
    let path = Path::new(path);
    if !path.is_file() {
        return Err(AppError::Message(format!(
            "file missing at last path: {path}. Choose the file again via Open.",
            path = path.display()
        )));
    }

    // If content changed, register as (possibly) same path new hash via open_and_register
    let (hash, _) = documents::content_hash(path)?;
    if hash != doc.content_hash {
        // Content changed: still open, may create new library entry
        return open_and_register(db, path);
    }
    open_and_register(db, path)
}
