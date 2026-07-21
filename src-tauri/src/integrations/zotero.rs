use crate::db::{with_conn, DbState};
use crate::error::{AppError, AppResult};
use crate::library;
use rusqlite::Connection;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct ZoteroItem {
    pub key: String,
    pub title: String,
    pub item_type: String,
    pub authors: String,
    pub year: Option<String>,
    pub attachment_path: Option<String>,
}

/// Import selectable items from a local Zotero SQLite DB (user picks zotero.sqlite).
/// Zotero must not lock the DB exclusively — we open read-only URI if possible.
pub fn list_items(zotero_db_path: &str, limit: usize) -> AppResult<Vec<ZoteroItem>> {
    let path = Path::new(zotero_db_path);
    if !path.is_file() {
        return Err(AppError::Message(format!(
            "Zotero DB not found: {zotero_db_path}"
        )));
    }

    // Try read-only
    let uri = format!("file:{}?mode=ro", path.display());
    let conn = Connection::open_with_flags(
        &uri,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .or_else(|_| Connection::open(path))
    .map_err(|e| AppError::Message(format!("open zotero.sqlite: {e}")))?;

    // Classic Zotero schema (items + itemData + fields + itemAttachments)
    let sql = r#"
        SELECT
          i.key,
          COALESCE(
            (SELECT idv.value FROM itemData id
              JOIN fields f ON f.fieldID = id.fieldID
              JOIN itemDataValues idv ON idv.valueID = id.valueID
             WHERE id.itemID = i.itemID AND f.fieldName = 'title' LIMIT 1),
            i.key
          ) AS title,
          COALESCE(t.typeName, 'item') AS typeName,
          COALESCE(
            (SELECT GROUP_CONCAT(c.lastName, ', ')
               FROM itemCreators ic
               JOIN creators c ON c.creatorID = ic.creatorID
              WHERE ic.itemID = i.itemID),
            ''
          ) AS authors,
          (SELECT idv.value FROM itemData id
            JOIN fields f ON f.fieldID = id.fieldID
            JOIN itemDataValues idv ON idv.valueID = id.valueID
           WHERE id.itemID = i.itemID AND f.fieldName = 'date' LIMIT 1) AS dateVal
        FROM items i
        LEFT JOIN itemTypes t ON t.itemTypeID = i.itemTypeID
        WHERE i.itemID NOT IN (SELECT itemID FROM deletedItems)
          AND COALESCE(t.typeName, '') NOT IN ('attachment', 'note', 'annotation')
        ORDER BY i.itemID DESC
        LIMIT ?1
    "#;

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| AppError::Message(format!(
            "Zotero schema query failed (is this a Zotero 5/6/7 database?): {e}"
        )))?;

    let rows = stmt
        .query_map([limit as i64], |row| {
            let date: Option<String> = row.get(4)?;
            let year = date.as_ref().and_then(|d| {
                d.chars()
                    .take(4)
                    .collect::<String>()
                    .parse::<u32>()
                    .ok()
                    .map(|y| y.to_string())
            });
            Ok(ZoteroItem {
                key: row.get(0)?,
                title: row.get(1)?,
                item_type: row.get(2)?,
                authors: row.get(3)?,
                year,
                attachment_path: None,
            })
        })
        .map_err(|e| AppError::Message(format!("zotero query: {e}")))?;

    let mut items = Vec::new();
    for r in rows {
        items.push(r?);
    }

    // Resolve first PDF attachment path when possible
    let data_dir = path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let storage = data_dir.join("storage");

    for item in &mut items {
        if let Ok(att) = first_attachment_path(&conn, &item.key, &storage) {
            item.attachment_path = att;
        }
    }

    Ok(items)
}

fn first_attachment_path(
    conn: &Connection,
    parent_key: &str,
    storage: &Path,
) -> AppResult<Option<String>> {
    let sql = r#"
        SELECT ia.path, child.key
        FROM items parent
        JOIN itemAttachments ia ON ia.parentItemID = parent.itemID
        JOIN items child ON child.itemID = ia.itemID
        WHERE parent.key = ?1
        LIMIT 5
    "#;
    let mut stmt = conn.prepare(sql)?;
    let mut rows = stmt.query([parent_key])?;
    while let Some(row) = rows.next()? {
        let path: Option<String> = row.get(0)?;
        let child_key: String = row.get(1)?;
        if let Some(p) = path {
            // Zotero storage: "storage:filename" or absolute
            if let Some(rest) = p.strip_prefix("storage:") {
                let full = storage.join(&child_key).join(rest);
                if full.is_file() {
                    return Ok(Some(full.to_string_lossy().to_string()));
                }
            } else {
                let full = PathBuf::from(&p);
                if full.is_file() {
                    return Ok(Some(p));
                }
            }
        }
    }
    Ok(None)
}

/// Import attachment files into SoheiDesk library (open_and_register).
pub fn import_attachments(
    db: &DbState,
    paths: Vec<String>,
) -> AppResult<Vec<library::DocumentRecord>> {
    let mut out = Vec::new();
    for p in paths {
        let path = Path::new(&p);
        if !path.is_file() {
            continue;
        }
        // only supported types
        if library::open_and_register(db, path).is_ok() {
            if let Ok(list) = library::list_documents(db) {
                if let Some(doc) = list.into_iter().find(|d| d.last_path.as_deref() == Some(&p)) {
                    out.push(doc);
                    continue;
                }
            }
        }
        // fallback register
        match library::open_and_register(db, path) {
            Ok(r) => out.push(r.document),
            Err(_) => continue,
        }
    }
    Ok(out)
}

/// Store last used zotero path in settings.
pub fn save_zotero_path(db: &DbState, path: &str) -> AppResult<()> {
    with_conn(db, |conn| {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('zotero_db_path', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [path],
        )?;
        Ok(())
    })
}
