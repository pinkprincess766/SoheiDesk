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

fn open_with_migrations<F>(path: &std::path::Path, before_migration: F) -> AppResult<Connection>
where
    F: FnMut(&Connection, i64, i64) -> AppResult<()>,
{
    let mut conn = Connection::open(path)?;
    // Validate before journal_mode changes persist anything to an existing
    // database. Unknown or malformed schemas must remain untouched.
    migrations::validate_compatible(&conn)?;
    configure(&conn)?;
    migrations::apply_with_hook(&mut conn, before_migration)?;
    Ok(conn)
}

#[cfg(test)]
pub fn open(path: &std::path::Path) -> AppResult<Connection> {
    open_with_migrations(path, |_, _, _| Ok(()))
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
    // Existing data must be recoverable before each individual schema step.
    // A brand-new version-0 database is empty, so no pre-migration copy is made.
    let conn = open_with_migrations(&db_path, |connection, _from, target| {
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

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "soheidesk-db-test-{}",
                uuid::Uuid::new_v4().simple()
            ));
            std::fs::create_dir(&path).expect("test directory");
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn future_schema_is_rejected_before_persistent_configuration() {
        let directory = TestDir::new();
        let database = directory.0.join("future.sqlite");
        let conn = Connection::open(&database).expect("database");
        conn.execute_batch(
            "CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
             );
             CREATE TABLE future_data(value TEXT NOT NULL);
             INSERT INTO future_data(value) VALUES ('preserve me');",
        )
        .expect("future schema");
        for version in 1..=migrations::latest_version() + 1 {
            conn.execute(
                "INSERT INTO schema_migrations(version, applied_at) VALUES (?1, ?2)",
                rusqlite::params![version, chrono::Utc::now().to_rfc3339()],
            )
            .expect("version marker");
        }
        let original_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("journal mode");
        assert_eq!(original_mode, "delete");
        drop(conn);

        let error = open(&database).expect_err("future schema must fail before configure");
        assert!(error.to_string().contains("newer than this app supports"));

        let conn = Connection::open(&database).expect("reopen database");
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("journal mode after refusal");
        let value: String = conn
            .query_row("SELECT value FROM future_data", [], |row| row.get(0))
            .expect("future data");
        assert_eq!(mode, "delete");
        assert_eq!(value, "preserve me");
    }

    #[test]
    fn existing_database_is_archived_before_migration() {
        let directory = TestDir::new();
        let database = directory.0.join("soheidesk.sqlite");
        let mut conn = Connection::open(&database).expect("database");
        configure(&conn).expect("configure");
        migrations::apply_to_version(&mut conn, 3).expect("version 3 schema");
        conn.execute(
            "INSERT INTO settings(key, value) VALUES ('migration_sentinel', 'preserve')",
            [],
        )
        .expect("sentinel");
        drop(conn);

        let conn = open_with_migrations(&database, |connection, _from, target| {
            crate::backup::create_archive(
                &directory.0,
                connection,
                crate::backup::BackupKind::PreMigration,
                Some(target),
            )?;
            Ok(())
        })
        .expect("open with migration backup");

        let backups = crate::backup::list_backups(&directory.0).expect("backups");
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].kind, crate::backup::BackupKind::PreMigration);
        assert_eq!(backups[0].schema_version, 3);
        assert!(backups[0].readable);
        assert_eq!(migrations::current_version(&conn).expect("version"), 4);
    }
}
