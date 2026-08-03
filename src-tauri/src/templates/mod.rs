use crate::atomic_file;
use crate::db::{with_conn, DbState};
use crate::error::{AppError, AppResult};
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateField {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateRecord {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub is_builtin: bool,
    pub fields_json: String,
    pub body_md: String,
    pub default_tags_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct TemplateInput {
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub fields: Vec<TemplateField>,
    pub body_md: String,
    pub default_tags: Option<Vec<String>>,
}

pub fn validate_fields(fields: &[TemplateField]) -> AppResult<()> {
    let mut keys = std::collections::HashSet::new();
    let allowed = ["text", "number", "date", "tags", "file", "textarea"];
    for f in fields {
        if f.key.trim().is_empty() {
            return Err(AppError::Message("field key must not be empty".into()));
        }
        if f.label.trim().is_empty() {
            return Err(AppError::Message(format!(
                "field '{}' needs a label",
                f.key
            )));
        }
        if !allowed.contains(&f.field_type.as_str()) {
            return Err(AppError::Message(format!(
                "invalid field type '{}' for key '{}'",
                f.field_type, f.key
            )));
        }
        if !keys.insert(f.key.clone()) {
            return Err(AppError::Message(format!(
                "duplicate field key '{}'",
                f.key
            )));
        }
    }
    Ok(())
}

pub fn seed_builtins(db: &DbState) -> AppResult<()> {
    with_conn(db, |conn| {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM templates WHERE is_builtin = 1",
            [],
            |r| r.get(0),
        )?;
        if count > 0 {
            return Ok(());
        }

        let now = Utc::now().to_rfc3339();
        let seeds: Vec<(&str, &str, &str, &str, &str)> = vec![
            (
                "free-note",
                "Свободная заметка",
                "Заметка без структуры",
                "general",
                r#"[]"#,
            ),
            (
                "synthesis",
                "Протокол / синтез",
                "Описание синтеза или протокола",
                "lab",
                r#"[
                  {"key":"compound","label":"Вещество","type":"text","required":true},
                  {"key":"conditions","label":"Условия","type":"textarea","required":false},
                  {"key":"procedure","label":"Процедура","type":"textarea","required":true},
                  {"key":"result","label":"Результат","type":"textarea","required":false},
                  {"key":"notes","label":"Заметки","type":"textarea","required":false}
                ]"#,
            ),
            (
                "measurement",
                "Измерение",
                "Запись измерения / спектра",
                "lab",
                r#"[
                  {"key":"sample","label":"Образец","type":"text","required":true},
                  {"key":"instrument","label":"Прибор","type":"text","required":false},
                  {"key":"params","label":"Параметры","type":"textarea","required":false},
                  {"key":"spectrum","label":"Файл спектра","type":"file","required":false},
                  {"key":"result","label":"Результат","type":"textarea","required":false},
                  {"key":"conclusion","label":"Вывод","type":"textarea","required":false}
                ]"#,
            ),
            (
                "paper-notes",
                "Конспект статьи",
                "Чтение и выжимка статьи",
                "reading",
                r#"[
                  {"key":"citation","label":"Цитирование","type":"text","required":true},
                  {"key":"question","label":"Вопрос / цель","type":"textarea","required":false},
                  {"key":"theses","label":"Тезисы","type":"textarea","required":true},
                  {"key":"quotes","label":"Цитаты","type":"textarea","required":false},
                  {"key":"followup","label":"Follow-up","type":"textarea","required":false}
                ]"#,
            ),
            (
                "weekly",
                "Еженедельный обзор",
                "Итоги недели",
                "planning",
                r#"[
                  {"key":"goals","label":"Цели","type":"textarea","required":false},
                  {"key":"done","label":"Сделано","type":"textarea","required":true},
                  {"key":"blockers","label":"Блокеры","type":"textarea","required":false},
                  {"key":"plan","label":"План","type":"textarea","required":false}
                ]"#,
            ),
        ];

        let bodies: std::collections::HashMap<&str, &str> = [
            ("free-note", "<!-- тело заметки -->\n\n"),
            (
                "synthesis",
                "## Процедура\n\n{{procedure}}\n\n## Результат\n\n{{result}}\n\n## Заметки\n\n{{notes}}\n",
            ),
            (
                "measurement",
                "## Образец\n\n{{sample}}\n\n## Параметры\n\n{{params}}\n\n## Результат\n\n{{result}}\n\n## Вывод\n\n{{conclusion}}\n",
            ),
            (
                "paper-notes",
                "## Вопрос\n\n{{question}}\n\n## Тезисы\n\n{{theses}}\n\n## Цитаты\n\n{{quotes}}\n\n## Follow-up\n\n{{followup}}\n",
            ),
            (
                "weekly",
                "## Цели\n\n{{goals}}\n\n## Сделано\n\n{{done}}\n\n## Блокеры\n\n{{blockers}}\n\n## План\n\n{{plan}}\n",
            ),
        ]
        .into_iter()
        .collect();

        for (id, name, desc, cat, fields) in seeds {
            let body = bodies.get(id).copied().unwrap_or("");
            conn.execute(
                "INSERT INTO templates (id, name, description, category, is_builtin, fields_json, body_md, default_tags_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, '[]', ?7, ?7)",
                params![id, name, desc, cat, fields, body, now],
            )?;
        }
        Ok(())
    })
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TemplateRecord> {
    Ok(TemplateRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        category: row.get(3)?,
        is_builtin: row.get::<_, i64>(4)? == 1,
        fields_json: row.get(5)?,
        body_md: row.get(6)?,
        default_tags_json: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

pub fn list_templates(db: &DbState) -> AppResult<Vec<TemplateRecord>> {
    with_conn(db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, description, category, is_builtin, fields_json, body_md, default_tags_json, created_at, updated_at
             FROM templates ORDER BY is_builtin DESC, name ASC",
        )?;
        let rows = stmt.query_map([], map_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
}

pub fn get_template(db: &DbState, id: &str) -> AppResult<TemplateRecord> {
    with_conn(db, |conn| {
        conn.query_row(
            "SELECT id, name, description, category, is_builtin, fields_json, body_md, default_tags_json, created_at, updated_at
             FROM templates WHERE id = ?1",
            params![id],
            map_row,
        )
        .map_err(|_| AppError::Message("template not found".into()))
    })
}

pub fn create_template(db: &DbState, input: TemplateInput) -> AppResult<TemplateRecord> {
    validate_fields(&input.fields)?;
    if input.name.trim().is_empty() {
        return Err(AppError::Message("template name required".into()));
    }
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let fields_json = serde_json::to_string(&input.fields)?;
    let tags = serde_json::to_string(&input.default_tags.unwrap_or_default())?;

    with_conn(db, |conn| {
        conn.execute(
            "INSERT INTO templates (id, name, description, category, is_builtin, fields_json, body_md, default_tags_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 0, ?5, ?6, ?7, ?8, ?8)",
            params![
                id,
                input.name.trim(),
                input.description,
                input.category,
                fields_json,
                input.body_md,
                tags,
                now
            ],
        )?;
        Ok(())
    })?;
    get_template(db, &id)
}

pub fn update_template(db: &DbState, id: &str, input: TemplateInput) -> AppResult<TemplateRecord> {
    let existing = get_template(db, id)?;
    if existing.is_builtin {
        return Err(AppError::Message(
            "builtin templates are immutable; duplicate to customize".into(),
        ));
    }
    validate_fields(&input.fields)?;
    let now = Utc::now().to_rfc3339();
    let fields_json = serde_json::to_string(&input.fields)?;
    let tags = serde_json::to_string(&input.default_tags.unwrap_or_default())?;

    with_conn(db, |conn| {
        conn.execute(
            "UPDATE templates SET name=?1, description=?2, category=?3, fields_json=?4, body_md=?5, default_tags_json=?6, updated_at=?7
             WHERE id=?8",
            params![
                input.name.trim(),
                input.description,
                input.category,
                fields_json,
                input.body_md,
                tags,
                now,
                id
            ],
        )?;
        Ok(())
    })?;
    get_template(db, id)
}

pub fn delete_template(db: &DbState, id: &str) -> AppResult<()> {
    let existing = get_template(db, id)?;
    if existing.is_builtin {
        return Err(AppError::Message("cannot delete builtin template".into()));
    }
    with_conn(db, |conn| {
        conn.execute("DELETE FROM templates WHERE id = ?1", params![id])?;
        Ok(())
    })
}

pub fn parse_fields(json: &str) -> AppResult<Vec<TemplateField>> {
    Ok(serde_json::from_str(json)?)
}

/// Portable template file format (JSON).
#[derive(Debug, Serialize, Deserialize)]
pub struct TemplateFile {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub fields: Vec<TemplateField>,
    pub body_md: String,
    pub default_tags: Option<Vec<String>>,
}

pub fn export_template_to_path(db: &DbState, id: &str, path: &str) -> AppResult<()> {
    let t = get_template(db, id)?;
    let fields = parse_fields(&t.fields_json)?;
    let tags: Vec<String> = t
        .default_tags_json
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    let file = TemplateFile {
        format: "soheidesk-template".into(),
        version: 1,
        name: t.name,
        description: t.description,
        category: t.category,
        fields,
        body_md: t.body_md,
        default_tags: Some(tags),
    };
    let json = serde_json::to_string_pretty(&file)?;
    atomic_file::write_bytes(path, json.as_bytes())
}

pub fn import_template_from_path(db: &DbState, path: &str) -> AppResult<TemplateRecord> {
    let raw = std::fs::read_to_string(path)?;
    let file: TemplateFile = serde_json::from_str(&raw)
        .map_err(|e| AppError::Message(format!("invalid template file: {e}")))?;
    if file.format != "soheidesk-template" && file.format != "soheidesk.template" {
        // still accept if name+fields present
    }
    create_template(
        db,
        TemplateInput {
            name: file.name,
            description: file.description,
            category: file.category.or_else(|| Some("imported".into())),
            fields: file.fields,
            body_md: file.body_md,
            default_tags: file.default_tags,
        },
    )
}
