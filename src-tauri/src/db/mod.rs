pub(crate) mod migrations;

use crate::error::{AppError, AppResult};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub struct DbState {
    pub conn: Mutex<Connection>,
    /// Serializes app-managed media writes with backup/restore. When both locks
    /// are needed, acquire `media` before `conn` to avoid lock-order deadlocks.
    pub media: Mutex<()>,
    pub data_dir: PathBuf,
}

pub fn data_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Message(format!("app data dir: {e}")))?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(test)]
pub fn open(path: &std::path::Path) -> AppResult<Connection> {
    let conn = Connection::open(path)?;
    configure(&conn)?;
    migrations::apply(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;
        ",
    )?;
    Ok(())
}

pub fn init(app: &AppHandle) -> AppResult<DbState> {
    let data_dir = data_dir(app)?;
    let db_path = data_dir.join("soheidesk.sqlite");
    let conn = Connection::open(&db_path)?;
    configure(&conn)?;
    // Existing data must be recoverable before each individual schema step.
    // A brand-new version-0 database is empty, so no pre-migration copy is made.
    migrations::apply_with_hook(&conn, |connection, _from, target| {
        crate::backup::create_archive(
            &data_dir,
            connection,
            crate::backup::BackupKind::PreMigration,
            Some(target),
        )?;
        Ok(())
    })?;
    Ok(DbState {
        conn: Mutex::new(conn),
        media: Mutex::new(()),
        data_dir,
    })
}

pub fn with_conn<F, T>(state: &DbState, f: F) -> AppResult<T>
where
    F: FnOnce(&Connection) -> AppResult<T>,
{
    let conn = state
        .conn
        .lock()
        .map_err(|_| AppError::Message("database lock poisoned".into()))?;
    f(&conn)
}
