use crate::db::{with_conn, DbState};
use crate::error::AppResult;
use rusqlite::params;
use tauri::State;

#[tauri::command]
pub fn get_setting(db: State<'_, DbState>, key: String) -> AppResult<Option<String>> {
    with_conn(&db, |conn| {
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    })
}

#[tauri::command]
pub fn set_setting(db: State<'_, DbState>, key: String, value: String) -> AppResult<()> {
    with_conn(&db, |conn| {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    })
}

#[tauri::command]
pub fn delete_setting(db: State<'_, DbState>, key: String) -> AppResult<()> {
    with_conn(&db, |conn| {
        conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        Ok(())
    })
}
