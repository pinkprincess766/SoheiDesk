mod migrations;

use crate::error::{AppError, AppResult};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub struct DbState {
    pub conn: Mutex<Connection>,
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

pub fn open(path: &Path) -> AppResult<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;
        ",
    )?;
    migrations::apply(&conn)?;
    Ok(conn)
}

pub fn init(app: &AppHandle) -> AppResult<DbState> {
    let data_dir = data_dir(app)?;
    let db_path = data_dir.join("soheidesk.sqlite");
    let conn = open(&db_path)?;
    Ok(DbState {
        conn: Mutex::new(conn),
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
