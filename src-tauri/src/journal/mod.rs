use crate::db::{with_conn, DbState};
use crate::error::{AppError, AppResult};
use crate::templates::{self, TemplateField, TemplateRecord};
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub id: String,
    pub title: String,
    pub template_id: Option<String>,
    pub template_snapshot_json: Option<String>,
    pub body_md: String,
    pub fields_json: Option<String>,
    pub tags_json: Option<String>,
    pub entry_date: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct JournalEntryInput {
    pub title: String,
    pub template_id: Option<String>,
    pub body_md: String,
    pub fields: Option<Map<String, Value>>,
    pub tags: Option<Vec<String>>,
    pub entry_date: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExportPreview {
    pub markdown: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalDraft {
    pub draft_key: String,
    pub entry_id: Option<String>,
    pub payload: Value,
    pub base_updated_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct JournalDraftInput {
    pub draft_key: String,
    pub entry_id: Option<String>,
    pub payload: Value,
    pub base_updated_at: Option<String>,
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<JournalEntry> {
    Ok(JournalEntry {
        id: row.get(0)?,
        title: row.get(1)?,
        template_id: row.get(2)?,
        template_snapshot_json: row.get(3)?,
        body_md: row.get(4)?,
        fields_json: row.get(5)?,
        tags_json: row.get(6)?,
        entry_date: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

pub fn list_entries(db: &DbState) -> AppResult<Vec<JournalEntry>> {
    with_conn(db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, template_id, template_snapshot_json, body_md, fields_json, tags_json, entry_date, created_at, updated_at
             FROM journal_entries ORDER BY entry_date DESC, updated_at DESC",
        )?;
        let rows = stmt.query_map([], map_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
}

pub fn get_entry(db: &DbState, id: &str) -> AppResult<JournalEntry> {
    with_conn(db, |conn| {
        conn.query_row(
            "SELECT id, title, template_id, template_snapshot_json, body_md, fields_json, tags_json, entry_date, created_at, updated_at
             FROM journal_entries WHERE id = ?1",
            params![id],
            map_row,
        )
        .map_err(|_| AppError::Message("journal entry not found".into()))
    })
}

fn fields_schema_from_entry(
    entry_fields_snapshot: &Option<String>,
    template: Option<&TemplateRecord>,
) -> AppResult<Vec<TemplateField>> {
    if let Some(snap) = entry_fields_snapshot {
        if let Ok(v) = serde_json::from_str::<Value>(snap) {
            if let Some(fields) = v.get("fields") {
                return Ok(serde_json::from_value(fields.clone())?);
            }
        }
    }
    if let Some(t) = template {
        return templates::parse_fields(&t.fields_json);
    }
    Ok(vec![])
}

pub fn validate_entry_fields(
    fields_schema: &[TemplateField],
    values: &Map<String, Value>,
) -> AppResult<()> {
    for f in fields_schema {
        if !f.required {
            continue;
        }
        let val = values.get(&f.key);
        let empty = match val {
            None => true,
            Some(Value::Null) => true,
            Some(Value::String(s)) => s.trim().is_empty(),
            Some(Value::Array(a)) => a.is_empty(),
            _ => false,
        };
        if empty {
            return Err(AppError::Message(format!(
                "required field '{}' ({}) is empty",
                f.label, f.key
            )));
        }
        if f.field_type == "number" {
            if let Some(Value::String(s)) = val {
                if !s.trim().is_empty() && s.parse::<f64>().is_err() {
                    return Err(AppError::Message(format!(
                        "field '{}' must be a number",
                        f.label
                    )));
                }
            }
        }
    }
    Ok(())
}

fn apply_placeholders(body: &str, fields: &Map<String, Value>) -> String {
    let mut out = body.to_string();
    for (k, v) in fields {
        let needle = format!("{{{{{k}}}}}");
        let rep = match v {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => String::new(),
            other => other.to_string(),
        };
        out = out.replace(&needle, &rep);
    }
    out
}

pub fn create_entry(db: &DbState, input: JournalEntryInput) -> AppResult<JournalEntry> {
    if input.title.trim().is_empty() {
        return Err(AppError::Message("title is required".into()));
    }

    let template = match &input.template_id {
        Some(id) => Some(templates::get_template(db, id)?),
        None => None,
    };

    let fields_map = input.fields.unwrap_or_default();
    let schema = if let Some(ref t) = template {
        templates::parse_fields(&t.fields_json)?
    } else {
        vec![]
    };
    validate_entry_fields(&schema, &fields_map)?;

    let snapshot = template.as_ref().map(|t| {
        serde_json::json!({
            "id": t.id,
            "name": t.name,
            "fields": schema,
            "body_md": t.body_md,
        })
        .to_string()
    });

    let mut body = input.body_md;
    if body.trim().is_empty() {
        if let Some(ref t) = template {
            body = apply_placeholders(&t.body_md, &fields_map);
        }
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let entry_date = input
        .entry_date
        .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
    let fields_json = serde_json::to_string(&fields_map)?;
    let tags_json = serde_json::to_string(&input.tags.unwrap_or_default())?;

    with_conn(db, |conn| {
        conn.execute(
            "INSERT INTO journal_entries (id, title, template_id, template_snapshot_json, body_md, fields_json, tags_json, entry_date, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            params![
                id,
                input.title.trim(),
                input.template_id,
                snapshot,
                body,
                fields_json,
                tags_json,
                entry_date,
                now
            ],
        )?;
        Ok(())
    })?;
    get_entry(db, &id)
}

pub fn update_entry(db: &DbState, id: &str, input: JournalEntryInput) -> AppResult<JournalEntry> {
    let existing = get_entry(db, id)?;
    if input.title.trim().is_empty() {
        return Err(AppError::Message("title is required".into()));
    }

    let template = match &input.template_id {
        Some(tid) => templates::get_template(db, tid).ok(),
        None => None,
    };
    let fields_map = input.fields.unwrap_or_default();
    let schema = fields_schema_from_entry(&existing.template_snapshot_json, template.as_ref())?;
    validate_entry_fields(&schema, &fields_map)?;

    let now = Utc::now().to_rfc3339();
    let entry_date = input.entry_date.unwrap_or(existing.entry_date);
    let fields_json = serde_json::to_string(&fields_map)?;
    let tags_json = serde_json::to_string(&input.tags.unwrap_or_default())?;

    with_conn(db, |conn| {
        conn.execute(
            "UPDATE journal_entries SET title=?1, template_id=?2, body_md=?3, fields_json=?4, tags_json=?5, entry_date=?6, updated_at=?7
             WHERE id=?8",
            params![
                input.title.trim(),
                input.template_id,
                input.body_md,
                fields_json,
                tags_json,
                entry_date,
                now,
                id
            ],
        )?;
        Ok(())
    })?;
    get_entry(db, id)
}

pub fn delete_entry(db: &DbState, id: &str) -> AppResult<()> {
    with_conn(db, |conn| {
        let n = conn.execute("DELETE FROM journal_entries WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(AppError::Message("journal entry not found".into()));
        }
        conn.execute(
            "DELETE FROM journal_drafts WHERE entry_id = ?1 OR draft_key = ?2",
            params![id, format!("entry:{id}")],
        )?;
        Ok(())
    })
}

fn map_draft_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<JournalDraft> {
    let payload_json: String = row.get(2)?;
    let payload = serde_json::from_str(&payload_json).unwrap_or(Value::Null);
    Ok(JournalDraft {
        draft_key: row.get(0)?,
        entry_id: row.get(1)?,
        payload,
        base_updated_at: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

pub fn get_draft(db: &DbState, draft_key: &str) -> AppResult<Option<JournalDraft>> {
    with_conn(db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT draft_key, entry_id, payload_json, base_updated_at, created_at, updated_at
             FROM journal_drafts WHERE draft_key = ?1",
        )?;
        let mut rows = stmt.query([draft_key])?;
        Ok(match rows.next()? {
            Some(row) => Some(map_draft_row(row)?),
            None => None,
        })
    })
}

pub fn list_drafts(db: &DbState) -> AppResult<Vec<JournalDraft>> {
    with_conn(db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT draft_key, entry_id, payload_json, base_updated_at, created_at, updated_at
             FROM journal_drafts ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], map_draft_row)?;
        let mut drafts = Vec::new();
        for row in rows {
            drafts.push(row?);
        }
        Ok(drafts)
    })
}

pub fn save_draft(db: &DbState, input: JournalDraftInput) -> AppResult<JournalDraft> {
    let key = input.draft_key.trim();
    if key.is_empty() || key.len() > 200 || input.payload.is_null() {
        return Err(AppError::Message("invalid journal draft".into()));
    }
    let payload_json = serde_json::to_string(&input.payload)?;
    // Protect IPC and the local database from accidentally storing enormous
    // pasted binaries. Attachments belong in the media store.
    if payload_json.len() > 2 * 1024 * 1024 {
        return Err(AppError::Message(
            "journal draft exceeds the 2 MiB safety limit".into(),
        ));
    }
    let now = Utc::now().to_rfc3339();
    with_conn(db, |conn| {
        conn.execute(
            "INSERT INTO journal_drafts
             (draft_key, entry_id, payload_json, base_updated_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(draft_key) DO UPDATE SET
               entry_id=excluded.entry_id,
               payload_json=excluded.payload_json,
               base_updated_at=excluded.base_updated_at,
               updated_at=excluded.updated_at",
            params![
                key,
                input.entry_id,
                payload_json,
                input.base_updated_at,
                now
            ],
        )?;
        Ok(())
    })?;
    get_draft(db, key)?.ok_or_else(|| AppError::Message("draft save failed".into()))
}

pub fn delete_draft(db: &DbState, draft_key: &str) -> AppResult<()> {
    with_conn(db, |conn| {
        conn.execute(
            "DELETE FROM journal_drafts WHERE draft_key = ?1",
            [draft_key],
        )?;
        Ok(())
    })
}

pub fn entry_to_markdown(entry: &JournalEntry) -> AppResult<String> {
    let fields: Map<String, Value> = entry
        .fields_json
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    let tags: Vec<String> = entry
        .tags_json
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    let mut md = String::new();
    md.push_str(&format!("# {}\n\n", entry.title));
    md.push_str(&format!("- **Date:** {}\n", entry.entry_date));
    if !tags.is_empty() {
        md.push_str(&format!("- **Tags:** {}\n", tags.join(", ")));
    }
    if !fields.is_empty() {
        md.push_str("\n## Fields\n\n");
        for (k, v) in &fields {
            let val = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            if !val.trim().is_empty() {
                md.push_str(&format!("- **{}:** {}\n", k, val));
            }
        }
    }
    md.push_str("\n## Body\n\n");
    md.push_str(&entry.body_md);
    md.push('\n');
    Ok(md)
}

pub fn preview_export(db: &DbState, id: &str) -> AppResult<ExportPreview> {
    let entry = get_entry(db, id)?;
    Ok(ExportPreview {
        title: entry.title.clone(),
        markdown: entry_to_markdown(&entry)?,
    })
}

pub fn export_entry_to_path(db: &DbState, id: &str, path: &str) -> AppResult<()> {
    let preview = preview_export(db, id)?;
    std::fs::write(path, preview.markdown)?;
    Ok(())
}

pub fn save_entry_as_template(
    db: &DbState,
    entry_id: &str,
    name: String,
) -> AppResult<TemplateRecord> {
    let entry = get_entry(db, entry_id)?;
    let fields = fields_schema_from_entry(&entry.template_snapshot_json, None).unwrap_or_default();
    let tags: Vec<String> = entry
        .tags_json
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    templates::create_template(
        db,
        templates::TemplateInput {
            name,
            description: Some(format!("From entry: {}", entry.title)),
            category: Some("custom".into()),
            fields,
            body_md: entry.body_md,
            default_tags: Some(tags),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    fn test_state() -> (DbState, std::path::PathBuf) {
        let path =
            std::env::temp_dir().join(format!("soheidesk-journal-{}.sqlite", Uuid::new_v4()));
        let conn = crate::db::open(&path).expect("test database");
        (
            DbState {
                conn: Mutex::new(conn),
                data_dir: std::env::temp_dir(),
            },
            path,
        )
    }

    #[test]
    fn markdown_export_includes_title_and_body() {
        let entry = JournalEntry {
            id: "1".into(),
            title: "Test entry".into(),
            template_id: None,
            template_snapshot_json: None,
            body_md: "Hello **world**".into(),
            fields_json: Some(json!({"sample": "A"}).to_string()),
            tags_json: Some(json!(["lab"]).to_string()),
            entry_date: "2026-07-21".into(),
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let md = entry_to_markdown(&entry).unwrap();
        assert!(md.contains("# Test entry"));
        assert!(md.contains("Hello **world**"));
        assert!(md.contains("sample"));
        assert!(md.contains("lab"));
    }

    #[test]
    fn required_field_validation() {
        let schema = vec![TemplateField {
            key: "sample".into(),
            label: "Образец".into(),
            field_type: "text".into(),
            required: true,
            default: None,
        }];
        let empty = Map::new();
        assert!(validate_entry_fields(&schema, &empty).is_err());
        let mut filled = Map::new();
        filled.insert("sample".into(), Value::String("x".into()));
        assert!(validate_entry_fields(&schema, &filled).is_ok());
    }

    #[test]
    fn journal_draft_roundtrip_update_and_delete() {
        let (db, path) = test_state();
        let first = save_draft(
            &db,
            JournalDraftInput {
                draft_key: "new".into(),
                entry_id: None,
                payload: json!({"title": "first", "body_md": "body"}),
                base_updated_at: None,
            },
        )
        .expect("save draft");
        assert_eq!(first.payload["title"], "first");

        let updated = save_draft(
            &db,
            JournalDraftInput {
                draft_key: "new".into(),
                entry_id: None,
                payload: json!({"title": "second", "body_md": "new body"}),
                base_updated_at: None,
            },
        )
        .expect("update draft");
        assert_eq!(updated.created_at, first.created_at);
        assert_eq!(
            get_draft(&db, "new")
                .expect("get draft")
                .expect("draft exists")
                .payload["title"],
            "second"
        );
        assert_eq!(list_drafts(&db).expect("list drafts").len(), 1);

        delete_draft(&db, "new").expect("delete draft");
        assert!(get_draft(&db, "new").expect("get deleted draft").is_none());
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn journal_draft_rejects_invalid_or_oversized_payload() {
        let (db, path) = test_state();
        assert!(save_draft(
            &db,
            JournalDraftInput {
                draft_key: String::new(),
                entry_id: None,
                payload: json!({"title": "x"}),
                base_updated_at: None,
            }
        )
        .is_err());
        assert!(save_draft(
            &db,
            JournalDraftInput {
                draft_key: "new".into(),
                entry_id: None,
                payload: json!({"body_md": "x".repeat(2 * 1024 * 1024 + 1)}),
                base_updated_at: None,
            }
        )
        .is_err());
        drop(db);
        let _ = std::fs::remove_file(path);
    }
}
