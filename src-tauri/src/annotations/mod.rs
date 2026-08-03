use crate::atomic_file;
use crate::db::{with_conn, DbState};
use crate::error::{AppError, AppResult};
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub id: String,
    pub document_id: String,
    pub ann_type: String,
    pub page: Option<i64>,
    pub position_json: String,
    pub content: Option<String>,
    pub color: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct AnnotationInput {
    pub document_id: String,
    pub ann_type: String,
    pub page: Option<i64>,
    pub position_json: String,
    pub content: Option<String>,
    pub color: Option<String>,
}

const ALLOWED_TYPES: &[&str] = &[
    "highlight",
    "comment",
    "drawing",
    "rect",
    "ellipse",
    "arrow",
];

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Annotation> {
    Ok(Annotation {
        id: row.get(0)?,
        document_id: row.get(1)?,
        ann_type: row.get(2)?,
        page: row.get(3)?,
        position_json: row.get(4)?,
        content: row.get(5)?,
        color: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

pub fn list_for_document(db: &DbState, document_id: &str) -> AppResult<Vec<Annotation>> {
    with_conn(db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, document_id, ann_type, page, position_json, content, color, created_at, updated_at
             FROM annotations WHERE document_id = ?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![document_id], map_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
}

pub fn create(db: &DbState, input: AnnotationInput) -> AppResult<Annotation> {
    if !ALLOWED_TYPES.contains(&input.ann_type.as_str()) {
        return Err(AppError::Message(format!(
            "ann_type must be one of: {}",
            ALLOWED_TYPES.join(", ")
        )));
    }
    let _: serde_json::Value = serde_json::from_str(&input.position_json)
        .map_err(|e| AppError::Message(format!("invalid position_json: {e}")))?;

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    with_conn(db, |conn| {
        conn.execute(
            "INSERT INTO annotations (id, document_id, ann_type, page, position_json, content, color, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![
                id,
                input.document_id,
                input.ann_type,
                input.page,
                input.position_json,
                input.content,
                input.color.unwrap_or_else(|| "#f7e07c".into()),
                now
            ],
        )?;
        Ok(())
    })?;
    get(db, &id)
}

pub fn get(db: &DbState, id: &str) -> AppResult<Annotation> {
    with_conn(db, |conn| {
        conn.query_row(
            "SELECT id, document_id, ann_type, page, position_json, content, color, created_at, updated_at
             FROM annotations WHERE id = ?1",
            params![id],
            map_row,
        )
        .map_err(|_| AppError::Message("annotation not found".into()))
    })
}

pub fn update(
    db: &DbState,
    id: &str,
    content: Option<String>,
    color: Option<String>,
) -> AppResult<Annotation> {
    let now = Utc::now().to_rfc3339();
    with_conn(db, |conn| {
        conn.execute(
            "UPDATE annotations SET content = COALESCE(?1, content), color = COALESCE(?2, color), updated_at = ?3
             WHERE id = ?4",
            params![content, color, now, id],
        )?;
        Ok(())
    })?;
    get(db, id)
}

pub fn delete(db: &DbState, id: &str) -> AppResult<()> {
    with_conn(db, |conn| {
        let n = conn.execute("DELETE FROM annotations WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(AppError::Message("annotation not found".into()));
        }
        Ok(())
    })
}

/// Export all annotations for a document as Markdown.
pub fn export_markdown(db: &DbState, document_id: &str, doc_title: &str) -> AppResult<String> {
    let anns = list_for_document(db, document_id)?;
    let mut md = String::new();
    md.push_str(&format!("# Annotations: {doc_title}\n\n"));
    md.push_str(&format!("_Document id:_ `{document_id}`\n\n"));
    if anns.is_empty() {
        md.push_str("_No annotations._\n");
        return Ok(md);
    }
    for (i, a) in anns.iter().enumerate() {
        md.push_str(&format!("## {}. {} ", i + 1, a.ann_type));
        if let Some(p) = a.page {
            md.push_str(&format!("(page {p})"));
        }
        md.push_str("\n\n");
        if let Some(c) = &a.color {
            md.push_str(&format!("- color: `{c}`\n"));
        }
        if let Some(c) = &a.content {
            if !c.is_empty() {
                md.push_str(&format!("- note: {c}\n"));
            }
        }
        // try quote from position
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&a.position_json) {
            if let Some(q) = v.get("quote").and_then(|x| x.as_str()) {
                md.push_str(&format!("\n> {}\n", q.replace('\n', " ")));
            }
            if let Some(rects) = v.get("rects") {
                md.push_str(&format!("\n```json\n{rects}\n```\n"));
            }
            if let Some(points) = v.get("points") {
                md.push_str(&format!(
                    "\n_drawing points:_ {}\n",
                    points.as_array().map(|a| a.len()).unwrap_or(0)
                ));
            }
        }
        md.push_str(&format!("\n_created:_ {}\n\n---\n\n", a.created_at));
    }
    Ok(md)
}

pub fn export_markdown_to_path(
    db: &DbState,
    document_id: &str,
    doc_title: &str,
    path: &str,
) -> AppResult<()> {
    let md = export_markdown(db, document_id, doc_title)?;
    atomic_file::write_bytes(path, md.as_bytes())
}
