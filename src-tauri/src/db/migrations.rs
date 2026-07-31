use crate::error::AppResult;
use rusqlite::Connection;

const MIGRATIONS: &[&str] = &[
    // 001 — foundation
    r#"
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
    // 002 — export templates + bibliography
    r#"
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
    // 003 — RSS + plugins
    r#"
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
    // 004 — crash-safe journal drafts
    r#"
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
];

pub fn apply(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );",
    )?;

    let current: i64 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;

    for (idx, sql) in MIGRATIONS.iter().enumerate() {
        let version = (idx + 1) as i64;
        if version <= current {
            continue;
        }
        conn.execute_batch(sql)?;
        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![version, chrono::Utc::now().to_rfc3339()],
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrades_v3_database_with_journal_drafts() {
        let conn = Connection::open_in_memory().expect("database");
        conn.execute_batch(
            "CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
             );
             INSERT INTO schema_migrations(version, applied_at)
             VALUES (1, 't'), (2, 't'), (3, 't');",
        )
        .expect("v3 schema marker");

        apply(&conn).expect("apply migration");
        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("schema version");
        let draft_table: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type='table' AND name='journal_drafts'",
                [],
                |row| row.get(0),
            )
            .expect("draft table");
        assert_eq!(version, 4);
        assert_eq!(draft_table, 1);
    }
}
