use crate::error::{AppError, AppResult};
use rusqlite::{Connection, TransactionBehavior};

#[derive(Clone, Copy)]
struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "foundation",
        sql: r#"
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS documents (
            id TEXT PRIMARY KEY,
            content_hash TEXT NOT NULL UNIQUE,
            title TEXT,
            last_path TEXT,
            doc_type TEXT NOT NULL,
            file_size INTEGER,
            added_at TEXT NOT NULL,
            last_opened_at TEXT,
            metadata_json TEXT
        );

        CREATE TABLE IF NOT EXISTS annotations (
            id TEXT PRIMARY KEY,
            document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
            ann_type TEXT NOT NULL,
            page INTEGER,
            position_json TEXT NOT NULL,
            content TEXT,
            color TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS templates (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            category TEXT,
            is_builtin INTEGER NOT NULL DEFAULT 0,
            fields_json TEXT NOT NULL,
            body_md TEXT NOT NULL,
            default_tags_json TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS journal_entries (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            template_id TEXT,
            template_snapshot_json TEXT,
            body_md TEXT NOT NULL,
            fields_json TEXT,
            tags_json TEXT,
            entry_date TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    },
    Migration {
        version: 2,
        name: "export templates and bibliography",
        sql: r#"
        CREATE TABLE IF NOT EXISTS export_templates (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            format TEXT NOT NULL,
            body TEXT NOT NULL,
            is_builtin INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS bibliography_items (
            id TEXT PRIMARY KEY,
            source TEXT NOT NULL,
            external_id TEXT,
            title TEXT NOT NULL,
            authors TEXT,
            year TEXT,
            journal TEXT,
            doi TEXT,
            url TEXT,
            bibtex TEXT,
            data_json TEXT,
            document_id TEXT,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_biblio_doi ON bibliography_items(doi);
        CREATE INDEX IF NOT EXISTS idx_biblio_source ON bibliography_items(source);
        "#,
    },
    Migration {
        version: 3,
        name: "RSS and plugins",
        sql: r#"
        CREATE TABLE IF NOT EXISTS rss_feeds (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            url TEXT NOT NULL UNIQUE,
            category TEXT,
            last_fetched_at TEXT,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS rss_items (
            id TEXT PRIMARY KEY,
            feed_id TEXT NOT NULL REFERENCES rss_feeds(id) ON DELETE CASCADE,
            guid TEXT,
            title TEXT NOT NULL,
            link TEXT,
            summary TEXT,
            published_at TEXT,
            is_read INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            UNIQUE(feed_id, guid)
        );

        CREATE TABLE IF NOT EXISTS plugins (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            command TEXT NOT NULL,
            args_json TEXT,
            extensions_json TEXT NOT NULL,
            description TEXT,
            enabled INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_rss_items_feed ON rss_items(feed_id);
        "#,
    },
    Migration {
        version: 4,
        name: "crash-safe journal drafts",
        sql: r#"
        CREATE TABLE IF NOT EXISTS journal_drafts (
            draft_key TEXT PRIMARY KEY,
            entry_id TEXT,
            payload_json TEXT NOT NULL,
            base_updated_at TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_journal_drafts_entry
            ON journal_drafts(entry_id);
        "#,
    },
    Migration {
        version: 5,
        name: "resilient document identity and annotation anchors",
        sql: r#"
        ALTER TABLE documents ADD COLUMN sha256 TEXT;
        CREATE UNIQUE INDEX IF NOT EXISTS idx_documents_sha256
            ON documents(sha256) WHERE sha256 IS NOT NULL;

        ALTER TABLE annotations ADD COLUMN selected_text TEXT;
        ALTER TABLE annotations ADD COLUMN context_before TEXT;
        ALTER TABLE annotations ADD COLUMN context_after TEXT;
        ALTER TABLE annotations ADD COLUMN anchor_status TEXT NOT NULL DEFAULT 'attached'
            CHECK(anchor_status IN ('attached', 'rebound', 'needs_review'));
        ALTER TABLE annotations ADD COLUMN source_sha256 TEXT;

        CREATE TABLE document_versions (
            id TEXT PRIMARY KEY,
            document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
            sha256 TEXT,
            legacy_hash TEXT,
            file_size INTEGER,
            path TEXT,
            title TEXT,
            change_kind TEXT NOT NULL CHECK(change_kind IN (
                'added', 'verified', 'moved', 'alternate_path',
                'content_changed', 'imported'
            )),
            observed_at TEXT NOT NULL
        );

        CREATE INDEX idx_document_versions_document
            ON document_versions(document_id, observed_at DESC);
        "#,
    },
];

fn table_exists(conn: &Connection, table: &str) -> AppResult<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_master
            WHERE type='table' AND name=?1
        )",
        [table],
        |row| row.get(0),
    )?)
}

fn validate_catalog(migrations: &[Migration]) -> AppResult<()> {
    if migrations.is_empty() {
        return Err(AppError::Message("migration catalog is empty".into()));
    }
    for (index, migration) in migrations.iter().enumerate() {
        let expected = (index + 1) as i64;
        if migration.version != expected || migration.name.trim().is_empty() {
            return Err(AppError::Message(format!(
                "invalid migration catalog entry at position {expected}"
            )));
        }
    }
    Ok(())
}

fn schema_version_for(conn: &Connection, migrations: &[Migration]) -> AppResult<i64> {
    validate_catalog(migrations)?;
    if !table_exists(conn, "schema_migrations")? {
        let schema_objects: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )?;
        if schema_objects != 0 {
            return Err(AppError::Message(
                "database has schema objects but no migration history; refusing to modify an unknown schema".into(),
            ));
        }
        return Ok(0);
    }

    let mut statement = conn.prepare(
        "SELECT version, applied_at
         FROM schema_migrations
         ORDER BY version",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut current = 0_i64;
    for row in rows {
        let (version, applied_at) = row?;
        let expected = current + 1;
        if version != expected || chrono::DateTime::parse_from_rfc3339(applied_at.trim()).is_err() {
            return Err(AppError::Message(format!(
                "invalid migration history at version {version}; expected version {expected} with a timestamp"
            )));
        }
        current = version;
    }
    if current == 0 {
        return Err(AppError::Message(
            "migration history table is empty; refusing to modify an ambiguous schema".into(),
        ));
    }

    let latest = migrations.last().expect("validated catalog").version;
    if current > latest {
        return Err(AppError::Message(format!(
            "database schema version {current} is newer than this app supports ({latest})"
        )));
    }
    Ok(current)
}

pub fn validate_compatible(conn: &Connection) -> AppResult<i64> {
    schema_version_for(conn, MIGRATIONS)
}

pub fn current_version(conn: &Connection) -> AppResult<i64> {
    validate_compatible(conn)
}

pub fn latest_version() -> i64 {
    MIGRATIONS
        .last()
        .expect("migration catalog is not empty")
        .version
}

fn quick_check(conn: &Connection) -> AppResult<()> {
    let result: String = conn.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    if result != "ok" {
        return Err(AppError::Message(format!(
            "SQLite quick_check failed: {result}"
        )));
    }
    Ok(())
}

#[cfg(test)]
pub fn apply(conn: &mut Connection) -> AppResult<()> {
    apply_with_hook(conn, |_, _, _| Ok(()))
}

#[cfg(test)]
pub fn apply_to_version(conn: &mut Connection, target: i64) -> AppResult<()> {
    if !(1..=latest_version()).contains(&target) {
        return Err(AppError::Message(format!(
            "unsupported test migration target {target}"
        )));
    }
    apply_catalog_with(
        conn,
        &MIGRATIONS[..target as usize],
        |_, _, _| Ok(()),
        |connection, _| quick_check(connection),
    )
}

pub fn apply_with_hook<F>(conn: &mut Connection, before_migration: F) -> AppResult<()>
where
    F: FnMut(&Connection, i64, i64) -> AppResult<()>,
{
    apply_catalog_with(conn, MIGRATIONS, before_migration, |connection, _| {
        quick_check(connection)
    })
}

fn apply_catalog_with<F, V>(
    conn: &mut Connection,
    migrations: &[Migration],
    mut before_migration: F,
    mut verify: V,
) -> AppResult<()>
where
    F: FnMut(&Connection, i64, i64) -> AppResult<()>,
    V: FnMut(&Connection, i64) -> AppResult<()>,
{
    let mut current = schema_version_for(conn, migrations)?;
    // A fresh database has no user data to protect. If startup resumes from a
    // real historical version, every remaining step receives its own backup.
    let requires_backup = current > 0;

    for migration in migrations {
        if migration.version <= current {
            continue;
        }
        if requires_backup {
            before_migration(conn, current, migration.version).map_err(|error| {
                AppError::Message(format!(
                    "pre-migration backup for schema {current} -> {} failed; database was not modified: {error}",
                    migration.version
                ))
            })?;
        }

        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| {
                AppError::Message(format!(
                    "migration {} ({}) could not start: {error}",
                    migration.version, migration.name
                ))
            })?;
        let migration_result = (|| -> AppResult<()> {
            transaction.execute_batch(migration.sql)?;
            transaction.execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                rusqlite::params![migration.version, chrono::Utc::now().to_rfc3339()],
            )?;
            // Run the check before commit so a failed verification rolls back
            // both schema changes and the version marker as one unit.
            verify(&transaction, migration.version)?;
            Ok(())
        })();

        if let Err(error) = migration_result {
            let rollback = transaction.rollback();
            return match rollback {
                Ok(()) => Err(AppError::Message(format!(
                    "migration {} ({}) failed and was rolled back: {error}",
                    migration.version, migration.name
                ))),
                Err(rollback_error) => Err(AppError::Message(format!(
                    "migration {} ({}) failed ({error}); rollback also failed: {rollback_error}",
                    migration.version, migration.name
                ))),
            };
        }

        if let Err(error) = transaction.execute_batch("COMMIT") {
            let rollback = transaction.rollback();
            return match rollback {
                Ok(()) => Err(AppError::Message(format!(
                    "migration {} ({}) could not commit and was rolled back: {error}",
                    migration.version, migration.name
                ))),
                Err(rollback_error) => Err(AppError::Message(format!(
                    "migration {} ({}) could not commit ({error}); rollback also failed: {rollback_error}",
                    migration.version, migration.name
                ))),
            };
        }
        drop(transaction);
        // Recheck the committed image as the final startup gate. The in-
        // transaction check provides rollback; this second check verifies the
        // exact database state that the rest of the application will open.
        quick_check(conn).map_err(|error| {
            AppError::Message(format!(
                "migration {} ({}) committed, but post-migration quick_check failed; startup stopped: {error}",
                migration.version, migration.name
            ))
        })?;
        current = migration.version;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database_at_version(version: i64) -> Connection {
        assert!((0..=latest_version()).contains(&version));
        let mut conn = Connection::open_in_memory().expect("database");
        if version > 0 {
            apply_catalog_with(
                &mut conn,
                &MIGRATIONS[..version as usize],
                |_, _, _| Ok(()),
                |connection, _| quick_check(connection),
            )
            .expect("historical schema");
        }
        conn
    }

    fn has_table(conn: &Connection, table: &str) -> bool {
        table_exists(conn, table).expect("table lookup")
    }

    #[test]
    fn upgrades_every_supported_historical_version() {
        for old_version in 0..latest_version() {
            let mut conn = database_at_version(old_version);
            if old_version >= 1 {
                conn.execute(
                    "INSERT INTO settings(key, value) VALUES ('migration_sentinel', 'preserved')",
                    [],
                )
                .expect("sentinel");
            }

            let mut backup_calls = Vec::new();
            apply_with_hook(&mut conn, |connection, from, to| {
                assert_eq!(current_version(connection).expect("current version"), from);
                backup_calls.push((from, to));
                Ok(())
            })
            .unwrap_or_else(|error| panic!("upgrade from version {old_version}: {error}"));

            assert_eq!(
                current_version(&conn).expect("latest version"),
                latest_version()
            );
            for table in [
                "settings",
                "documents",
                "annotations",
                "templates",
                "journal_entries",
                "export_templates",
                "bibliography_items",
                "rss_feeds",
                "rss_items",
                "plugins",
                "journal_drafts",
                "document_versions",
            ] {
                assert!(
                    has_table(&conn, table),
                    "missing {table} after v{old_version} upgrade"
                );
            }
            if old_version >= 1 {
                let sentinel: String = conn
                    .query_row(
                        "SELECT value FROM settings WHERE key='migration_sentinel'",
                        [],
                        |row| row.get(0),
                    )
                    .expect("preserved sentinel");
                assert_eq!(sentinel, "preserved");
                let expected: Vec<_> = (old_version..latest_version())
                    .map(|from| (from, from + 1))
                    .collect();
                assert_eq!(backup_calls, expected);
            } else {
                assert!(backup_calls.is_empty());
            }
            quick_check(&conn).expect("upgraded database");
        }
    }

    #[test]
    fn backup_hook_runs_before_schema_mutation() {
        let mut conn = database_at_version(4);
        apply_with_hook(&mut conn, |connection, from, to| {
            assert_eq!((from, to), (4, 5));
            assert!(!has_table(connection, "document_versions"));
            Ok(())
        })
        .expect("migration with backup hook");
        assert!(has_table(&conn, "document_versions"));
    }

    #[test]
    fn backup_failure_stops_before_schema_mutation() {
        let mut conn = database_at_version(3);

        let error = apply_with_hook(&mut conn, |_, _, _| {
            Err(AppError::Message("simulated backup failure".into()))
        })
        .expect_err("backup failure must abort migration");

        assert!(error.to_string().contains("database was not modified"));
        assert!(error.to_string().contains("simulated backup failure"));
        assert_eq!(current_version(&conn).expect("version"), 3);
        assert!(!has_table(&conn, "journal_drafts"));
    }

    #[test]
    fn rejects_schema_newer_than_the_application() {
        let mut conn = database_at_version(latest_version());
        conn.execute(
            "INSERT INTO schema_migrations(version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![latest_version() + 1, chrono::Utc::now().to_rfc3339()],
        )
        .expect("future marker");
        let mut backup_called = false;

        let error = apply_with_hook(&mut conn, |_, _, _| {
            backup_called = true;
            Ok(())
        })
        .expect_err("future schema must fail");

        assert!(error.to_string().contains("newer than this app supports"));
        assert!(!backup_called);
    }

    #[test]
    fn rejects_gapped_or_ambiguous_migration_history() {
        let gapped = database_at_version(latest_version());
        gapped
            .execute("DELETE FROM schema_migrations WHERE version=2", [])
            .expect("create history gap");
        assert!(current_version(&gapped)
            .expect_err("gapped history")
            .to_string()
            .contains("invalid migration history"));

        let invalid_timestamp = database_at_version(latest_version());
        invalid_timestamp
            .execute(
                "UPDATE schema_migrations SET applied_at='' WHERE version=2",
                [],
            )
            .expect("invalidate timestamp");
        assert!(current_version(&invalid_timestamp)
            .expect_err("invalid timestamp")
            .to_string()
            .contains("with a timestamp"));

        let mut unknown = Connection::open_in_memory().expect("database");
        unknown
            .execute("CREATE TABLE unknown_payload(value TEXT)", [])
            .expect("unknown table");
        assert!(apply(&mut unknown)
            .expect_err("unversioned schema")
            .to_string()
            .contains("no migration history"));
    }

    #[test]
    fn failed_sql_rolls_back_the_whole_migration() {
        let mut conn = database_at_version(1);
        conn.execute(
            "INSERT INTO settings(key, value) VALUES ('sentinel', 'safe')",
            [],
        )
        .expect("sentinel");
        let failing = Migration {
            version: 2,
            name: "intentional failure",
            sql: "CREATE TABLE partial_change(value TEXT);\n\
                  INSERT INTO table_that_does_not_exist(value) VALUES ('boom');",
        };
        let catalog = [MIGRATIONS[0], failing];

        let error = apply_catalog_with(
            &mut conn,
            &catalog,
            |_, _, _| Ok(()),
            |connection, _| quick_check(connection),
        )
        .expect_err("migration must fail");

        assert!(error.to_string().contains("failed and was rolled back"));
        assert!(!has_table(&conn, "partial_change"));
        assert_eq!(schema_version_for(&conn, &catalog).expect("version"), 1);
        let sentinel: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key='sentinel'",
                [],
                |row| row.get(0),
            )
            .expect("sentinel preserved");
        assert_eq!(sentinel, "safe");
    }

    #[test]
    fn failed_post_migration_check_rolls_back_schema_and_version() {
        let mut conn = database_at_version(1);
        let catalog = &MIGRATIONS[..2];

        let error = apply_catalog_with(
            &mut conn,
            catalog,
            |_, _, _| Ok(()),
            |_, version| {
                if version == 2 {
                    Err(AppError::Message("simulated quick_check failure".into()))
                } else {
                    Ok(())
                }
            },
        )
        .expect_err("verification must fail");

        assert!(error.to_string().contains("simulated quick_check failure"));
        assert!(!has_table(&conn, "export_templates"));
        assert_eq!(schema_version_for(&conn, catalog).expect("version"), 1);
    }

    #[test]
    fn failed_commit_is_explicitly_rolled_back() {
        let mut conn = database_at_version(1);
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .expect("foreign keys");
        let deferred_failure = Migration {
            version: 2,
            name: "deferred constraint failure",
            sql: "CREATE TABLE commit_parent(id INTEGER PRIMARY KEY);\n\
                  CREATE TABLE commit_child(\n\
                      parent_id INTEGER REFERENCES commit_parent(id)\n\
                          DEFERRABLE INITIALLY DEFERRED\n\
                  );\n\
                  INSERT INTO commit_child(parent_id) VALUES (99);",
        };
        let catalog = [MIGRATIONS[0], deferred_failure];

        let error = apply_catalog_with(
            &mut conn,
            &catalog,
            |_, _, _| Ok(()),
            |connection, _| quick_check(connection),
        )
        .expect_err("commit must fail");

        assert!(error
            .to_string()
            .contains("could not commit and was rolled back"));
        assert!(!has_table(&conn, "commit_parent"));
        assert!(!has_table(&conn, "commit_child"));
        assert_eq!(schema_version_for(&conn, &catalog).expect("version"), 1);
    }

    #[test]
    fn verifies_each_migration_before_commit() {
        let mut conn = Connection::open_in_memory().expect("database");
        let mut verified = Vec::new();

        apply_catalog_with(
            &mut conn,
            MIGRATIONS,
            |_, _, _| Ok(()),
            |connection, version| {
                quick_check(connection)?;
                verified.push(version);
                Ok(())
            },
        )
        .expect("verified migrations");

        assert_eq!(verified, (1..=latest_version()).collect::<Vec<_>>());
    }
}
