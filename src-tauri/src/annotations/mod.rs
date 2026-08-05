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
    pub selected_text: Option<String>,
    pub context_before: Option<String>,
    pub context_after: Option<String>,
    pub anchor_status: String,
    pub source_sha256: Option<String>,
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
    pub selected_text: Option<String>,
    pub context_before: Option<String>,
    pub context_after: Option<String>,
}

const ALLOWED_TYPES: &[&str] = &[
    "highlight",
    "comment",
    "drawing",
    "rect",
    "ellipse",
    "arrow",
];
const MAX_POSITION_BYTES: usize = 128 * 1024;
const MAX_CONTENT_BYTES: usize = 64 * 1024;
const MAX_SELECTED_TEXT_BYTES: usize = 32 * 1024;
const MAX_CONTEXT_BYTES: usize = 4 * 1024;
const DEFAULT_COLOR: &str = "#f7e07c";

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Annotation> {
    Ok(Annotation {
        id: row.get(0)?,
        document_id: row.get(1)?,
        ann_type: row.get(2)?,
        page: row.get(3)?,
        position_json: row.get(4)?,
        content: row.get(5)?,
        color: row.get(6)?,
        selected_text: row.get(7)?,
        context_before: row.get(8)?,
        context_after: row.get(9)?,
        anchor_status: row.get(10)?,
        source_sha256: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

pub fn list_for_document(db: &DbState, document_id: &str) -> AppResult<Vec<Annotation>> {
    with_conn(db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, document_id, ann_type, page, position_json, content, color,
                    selected_text, context_before, context_after, anchor_status, source_sha256,
                    created_at, updated_at
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
    validate_size("position_json", &input.position_json, MAX_POSITION_BYTES)?;
    validate_optional_size("content", input.content.as_deref(), MAX_CONTENT_BYTES)?;
    validate_optional_size(
        "selected_text",
        input.selected_text.as_deref(),
        MAX_SELECTED_TEXT_BYTES,
    )?;
    validate_optional_size(
        "context_before",
        input.context_before.as_deref(),
        MAX_CONTEXT_BYTES,
    )?;
    validate_optional_size(
        "context_after",
        input.context_after.as_deref(),
        MAX_CONTEXT_BYTES,
    )?;
    let position: serde_json::Value = serde_json::from_str(&input.position_json)
        .map_err(|e| AppError::Message(format!("invalid position_json: {e}")))?;
    if !position.is_object() {
        return Err(AppError::Message(
            "position_json must be a JSON object".into(),
        ));
    }
    let color = input.color.unwrap_or_else(|| DEFAULT_COLOR.into());
    validate_color(&color)?;

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    with_conn(db, |conn| {
        conn.execute(
            "INSERT INTO annotations (
                id, document_id, ann_type, page, position_json, content, color,
                selected_text, context_before, context_after, anchor_status, source_sha256,
                created_at, updated_at
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'attached',
                (SELECT sha256 FROM documents WHERE id = ?2), ?11, ?11
             )",
            params![
                id,
                input.document_id,
                input.ann_type,
                input.page,
                input.position_json,
                input.content,
                color,
                normalize(input.selected_text),
                normalize(input.context_before),
                normalize(input.context_after),
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
            "SELECT id, document_id, ann_type, page, position_json, content, color,
                    selected_text, context_before, context_after, anchor_status, source_sha256,
                    created_at, updated_at
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
    validate_optional_size("content", content.as_deref(), MAX_CONTENT_BYTES)?;
    if let Some(color) = color.as_deref() {
        validate_color(color)?;
    }
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
        if a.anchor_status == "needs_review" {
            md.push_str("- anchor: **Needs review**\n");
        } else if a.anchor_status == "rebound" {
            md.push_str("- anchor: rebound after document update\n");
        }
        if let Some(selected) = &a.selected_text {
            md.push_str(&format!("\n> {}\n", selected.replace('\n', " ")));
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

fn normalize(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}

fn validate_size(field: &str, value: &str, maximum: usize) -> AppResult<()> {
    if value.len() > maximum {
        return Err(AppError::Message(format!(
            "{field} is too large (maximum {maximum} bytes)"
        )));
    }
    Ok(())
}

fn validate_optional_size(field: &str, value: Option<&str>, maximum: usize) -> AppResult<()> {
    if let Some(value) = value {
        validate_size(field, value, maximum)?;
    }
    Ok(())
}

fn validate_color(color: &str) -> AppResult<()> {
    if matches!(color.len(), 7 | 9)
        && color.starts_with('#')
        && color[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Ok(());
    }
    Err(AppError::Message(
        "color must be a 6- or 8-digit hexadecimal value".into(),
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_bounded_hex_colors() {
        assert!(validate_color("#aabbcc").is_ok());
        assert!(validate_color("#aabbccdd").is_ok());
        assert!(validate_color("red; background:url(file:///tmp/x)").is_err());
        assert!(validate_color("#abcd").is_err());
    }

    #[test]
    fn rejects_oversized_anchor_fields() {
        let oversized = "x".repeat(MAX_CONTEXT_BYTES + 1);
        assert!(validate_optional_size("context", Some(&oversized), MAX_CONTEXT_BYTES).is_err());
    }
}
