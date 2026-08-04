use crate::db::{with_conn, with_conn_mut, DbState};
use crate::documents::{self, DocType};
use crate::error::{AppError, AppResult};
use crate::parsers;
use chrono::Utc;
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Serialize, Clone)]
pub struct DocumentRecord {
    pub id: String,
    pub content_hash: String,
    pub sha256: Option<String>,
    pub title: Option<String>,
    pub last_path: Option<String>,
    pub doc_type: String,
    pub file_size: Option<i64>,
    pub added_at: String,
    pub last_opened_at: Option<String>,
    pub version_count: i64,
}

#[derive(Debug, Serialize, Clone)]
pub struct DocumentVersion {
    pub id: String,
    pub document_id: String,
    pub sha256: Option<String>,
    pub legacy_hash: Option<String>,
    pub file_size: Option<i64>,
    pub path: Option<String>,
    pub title: Option<String>,
    pub change_kind: String,
    pub observed_at: String,
}

#[derive(Debug, Serialize)]
pub struct OpenResult {
    pub document: DocumentRecord,
    pub opened: parsers::OpenedDocument,
    pub movement_detected: bool,
    pub content_changed: bool,
    pub annotations_rebound: u64,
    pub annotations_needing_review: u64,
}

struct IdentityMatch {
    id: String,
    last_path: Option<String>,
    sha256: Option<String>,
    content_hash: String,
    file_size: Option<i64>,
    title: Option<String>,
}

pub fn list_documents(db: &DbState) -> AppResult<Vec<DocumentRecord>> {
    with_conn(db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT d.id, d.content_hash, d.sha256, d.title, d.last_path, d.doc_type,
                    d.file_size, d.added_at, d.last_opened_at,
                    (SELECT COUNT(*) FROM document_versions v WHERE v.document_id = d.id)
             FROM documents d
             ORDER BY COALESCE(d.last_opened_at, d.added_at) DESC",
        )?;
        let rows = stmt.query_map([], map_document)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    })
}

/// Open a path and update document identity, history, and anchors atomically.
pub fn open_and_register(db: &DbState, path: &Path) -> AppResult<OpenResult> {
    let _media = db
        .media
        .lock()
        .map_err(|_| AppError::Message("media lock poisoned".into()))?;
    let normalized_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let (sha256, file_size) = documents::content_hash(&normalized_path)?;
    let (legacy_hash, _) = documents::legacy_content_hash(&normalized_path)?;
    let cache_dir = db.data_dir.join("media").join(&sha256);
    let opened = parsers::open_document_with_identity(
        &normalized_path,
        Some(&cache_dir),
        sha256.clone(),
        file_size,
    )?;
    // Parsing can take long enough for another process to replace the source.
    // Refuse a mixed snapshot instead of attaching parsed content to the wrong digest.
    let (verified_sha256, verified_size) = documents::content_hash(&normalized_path)?;
    if verified_sha256 != sha256 || verified_size != file_size {
        return Err(AppError::Message(
            "document changed while it was being opened; no library records were modified".into(),
        ));
    }
    let doc_type = DocType::from_path(&normalized_path)?;
    let now = Utc::now().to_rfc3339();

    let (record, movement_detected, content_changed, rebound, needs_review) =
        with_conn_mut(db, |conn| {
            let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let outcome = register_in_transaction(
                &transaction,
                &opened,
                doc_type.as_str(),
                &sha256,
                &legacy_hash,
                file_size,
                &now,
            )?;
            transaction.commit()?;
            Ok(outcome)
        })?;

    Ok(OpenResult {
        document: record,
        opened,
        movement_detected,
        content_changed,
        annotations_rebound: rebound,
        annotations_needing_review: needs_review,
    })
}

#[allow(clippy::too_many_arguments)]
fn register_in_transaction(
    transaction: &Transaction<'_>,
    opened: &parsers::OpenedDocument,
    doc_type: &str,
    sha256: &str,
    legacy_hash: &str,
    file_size: u64,
    now: &str,
) -> AppResult<(DocumentRecord, bool, bool, u64, u64)> {
    let exact = transaction
        .query_row(
            "SELECT id, last_path, sha256, content_hash, file_size, title
             FROM documents
             WHERE sha256 = ?1 OR content_hash = ?1
                OR (sha256 IS NULL AND content_hash = ?2)
             ORDER BY CASE
                WHEN sha256 = ?1 THEN 0
                WHEN content_hash = ?1 THEN 1
                ELSE 2
             END
             LIMIT 1",
            params![sha256, legacy_hash],
            map_identity,
        )
        .optional()?;

    let same_path = transaction
        .query_row(
            "SELECT id, last_path, sha256, content_hash, file_size, title
             FROM documents WHERE last_path = ?1 LIMIT 1",
            params![opened.path],
            map_identity,
        )
        .optional()?;

    if let (Some(exact), Some(path_match)) = (exact.as_ref(), same_path.as_ref()) {
        if exact.id != path_match.id {
            return Err(AppError::Message(
                "the updated file matches another library document; no records were changed".into(),
            ));
        }
    }

    if let Some(existing) = exact {
        let moved = existing.last_path.as_deref() != Some(opened.path.as_str());
        let identity_upgraded =
            existing.sha256.as_deref() != Some(sha256) || existing.content_hash != sha256;
        let change_kind = if moved {
            if existing
                .last_path
                .as_deref()
                .is_some_and(|old| !Path::new(old).is_file())
            {
                Some("moved")
            } else {
                Some("alternate_path")
            }
        } else if identity_upgraded {
            Some("verified")
        } else {
            None
        };

        transaction.execute(
            "UPDATE documents
             SET content_hash = ?1, sha256 = ?1, last_path = ?2,
                 title = COALESCE(title, ?3), last_opened_at = ?4,
                 file_size = ?5, doc_type = ?6
             WHERE id = ?7",
            params![
                sha256,
                opened.path,
                opened.title,
                now,
                file_size as i64,
                doc_type,
                existing.id
            ],
        )?;
        if let Some(kind) = change_kind {
            insert_version(
                transaction,
                &existing.id,
                Some(sha256),
                identity_upgraded.then_some(legacy_hash),
                file_size as i64,
                &opened.path,
                &opened.title,
                kind,
                now,
            )?;
        }
        let record = get_document_in(transaction, &existing.id)?;
        return Ok((record, moved, false, 0, 0));
    }

    if let Some(existing) = same_path {
        ensure_previous_version(transaction, &existing, now)?;
        transaction.execute(
            "UPDATE documents
             SET content_hash = ?1, sha256 = ?1, title = ?2, last_opened_at = ?3,
                 file_size = ?4, doc_type = ?5
             WHERE id = ?6",
            params![
                sha256,
                opened.title,
                now,
                file_size as i64,
                doc_type,
                existing.id
            ],
        )?;
        insert_version(
            transaction,
            &existing.id,
            Some(sha256),
            None,
            file_size as i64,
            &opened.path,
            &opened.title,
            "content_changed",
            now,
        )?;
        let (rebound, needs_review) = rebind_annotations(
            transaction,
            &existing.id,
            opened.text.as_deref().unwrap_or(""),
            sha256,
            now,
        )?;
        let record = get_document_in(transaction, &existing.id)?;
        return Ok((record, false, true, rebound, needs_review));
    }

    let id = Uuid::new_v4().to_string();
    transaction.execute(
        "INSERT INTO documents (
            id, content_hash, sha256, title, last_path, doc_type, file_size,
            added_at, last_opened_at
         ) VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![
            id,
            sha256,
            opened.title,
            opened.path,
            doc_type,
            file_size as i64,
            now
        ],
    )?;
    insert_version(
        transaction,
        &id,
        Some(sha256),
        None,
        file_size as i64,
        &opened.path,
        &opened.title,
        "added",
        now,
    )?;
    let record = get_document_in(transaction, &id)?;
    Ok((record, false, false, 0, 0))
}

fn map_identity(row: &rusqlite::Row<'_>) -> rusqlite::Result<IdentityMatch> {
    Ok(IdentityMatch {
        id: row.get(0)?,
        last_path: row.get(1)?,
        sha256: row.get(2)?,
        content_hash: row.get(3)?,
        file_size: row.get(4)?,
        title: row.get(5)?,
    })
}

fn map_document(row: &rusqlite::Row<'_>) -> rusqlite::Result<DocumentRecord> {
    Ok(DocumentRecord {
        id: row.get(0)?,
        content_hash: row.get(1)?,
        sha256: row.get(2)?,
        title: row.get(3)?,
        last_path: row.get(4)?,
        doc_type: row.get(5)?,
        file_size: row.get(6)?,
        added_at: row.get(7)?,
        last_opened_at: row.get(8)?,
        version_count: row.get(9)?,
    })
}

fn get_document_in(transaction: &Transaction<'_>, id: &str) -> AppResult<DocumentRecord> {
    transaction
        .query_row(
            "SELECT d.id, d.content_hash, d.sha256, d.title, d.last_path, d.doc_type,
                    d.file_size, d.added_at, d.last_opened_at,
                    (SELECT COUNT(*) FROM document_versions v WHERE v.document_id = d.id)
             FROM documents d WHERE d.id = ?1",
            params![id],
            map_document,
        )
        .map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
fn insert_version(
    transaction: &Transaction<'_>,
    document_id: &str,
    sha256: Option<&str>,
    legacy_hash: Option<&str>,
    file_size: i64,
    path: &str,
    title: &str,
    change_kind: &str,
    observed_at: &str,
) -> AppResult<()> {
    transaction.execute(
        "INSERT INTO document_versions (
            id, document_id, sha256, legacy_hash, file_size, path, title,
            change_kind, observed_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            Uuid::new_v4().to_string(),
            document_id,
            sha256,
            legacy_hash,
            file_size,
            path,
            title,
            change_kind,
            observed_at
        ],
    )?;
    Ok(())
}

fn ensure_previous_version(
    transaction: &Transaction<'_>,
    existing: &IdentityMatch,
    now: &str,
) -> AppResult<()> {
    let count: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM document_versions WHERE document_id = ?1",
        params![existing.id],
        |row| row.get(0),
    )?;
    if count == 0 {
        insert_version(
            transaction,
            &existing.id,
            existing.sha256.as_deref(),
            existing
                .sha256
                .is_none()
                .then_some(existing.content_hash.as_str()),
            existing.file_size.unwrap_or(0),
            existing.last_path.as_deref().unwrap_or(""),
            existing.title.as_deref().unwrap_or("Untitled"),
            "verified",
            now,
        )?;
    }
    Ok(())
}

struct AnchorRow {
    id: String,
    page: Option<i64>,
    position_json: String,
    selected_text: Option<String>,
    context_before: Option<String>,
    context_after: Option<String>,
}

fn rebind_annotations(
    transaction: &Transaction<'_>,
    document_id: &str,
    text: &str,
    sha256: &str,
    now: &str,
) -> AppResult<(u64, u64)> {
    let anchors = {
        let mut statement = transaction.prepare(
            "SELECT id, page, position_json, selected_text, context_before, context_after
             FROM annotations WHERE document_id = ?1",
        )?;
        let rows = statement.query_map(params![document_id], |row| {
            Ok(AnchorRow {
                id: row.get(0)?,
                page: row.get(1)?,
                position_json: row.get(2)?,
                selected_text: row.get(3)?,
                context_before: row.get(4)?,
                context_after: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    let mut rebound = 0_u64;
    let mut needs_review = 0_u64;
    for anchor in anchors {
        let match_at = anchor.selected_text.as_deref().and_then(|selected| {
            find_unambiguous_anchor(
                text,
                selected,
                anchor.context_before.as_deref(),
                anchor.context_after.as_deref(),
            )
        });

        if let Some(byte_start) = match_at {
            let selected = anchor.selected_text.as_deref().unwrap_or_default();
            let start_offset = text[..byte_start].encode_utf16().count();
            let end_offset = start_offset + selected.encode_utf16().count();
            let Ok(mut position) = serde_json::from_str::<serde_json::Value>(&anchor.position_json)
            else {
                needs_review += 1;
                transaction.execute(
                    "UPDATE annotations SET anchor_status = 'needs_review', updated_at = ?1
                     WHERE id = ?2",
                    params![now, anchor.id],
                )?;
                continue;
            };
            let Some(object) = position.as_object_mut() else {
                needs_review += 1;
                transaction.execute(
                    "UPDATE annotations SET anchor_status = 'needs_review', updated_at = ?1
                     WHERE id = ?2",
                    params![now, anchor.id],
                )?;
                continue;
            };
            object.insert("text_start_offset".into(), start_offset.into());
            object.insert("text_end_offset".into(), end_offset.into());
            if anchor.page.is_none() {
                object.insert("start_offset".into(), start_offset.into());
                object.insert("end_offset".into(), end_offset.into());
                object.insert("quote".into(), selected.into());
            }

            // PDF geometry cannot be proven from extracted-text coordinates.
            // Preserve it for review, but only reflow anchors become trusted.
            let status = if anchor.page.is_none() {
                rebound += 1;
                "rebound"
            } else {
                needs_review += 1;
                "needs_review"
            };
            transaction.execute(
                "UPDATE annotations
                 SET position_json = ?1, anchor_status = ?2,
                     source_sha256 = CASE WHEN ?2 = 'rebound' THEN ?3 ELSE source_sha256 END,
                     updated_at = ?4
                 WHERE id = ?5",
                params![position.to_string(), status, sha256, now, anchor.id],
            )?;
        } else {
            needs_review += 1;
            transaction.execute(
                "UPDATE annotations SET anchor_status = 'needs_review', updated_at = ?1
                 WHERE id = ?2",
                params![now, anchor.id],
            )?;
        }
    }
    Ok((rebound, needs_review))
}

fn find_unambiguous_anchor(
    text: &str,
    selected: &str,
    context_before: Option<&str>,
    context_after: Option<&str>,
) -> Option<usize> {
    if selected.is_empty() {
        return None;
    }
    let matches = text
        .match_indices(selected)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if matches.len() == 1 {
        return matches.first().copied();
    }

    let contextual = matches
        .into_iter()
        .filter(|index| {
            let before_matches = context_before
                .filter(|value| !value.is_empty())
                .is_none_or(|value| text[..*index].ends_with(value));
            let after_index = *index + selected.len();
            let after_matches = context_after
                .filter(|value| !value.is_empty())
                .is_none_or(|value| text[after_index..].starts_with(value));
            before_matches && after_matches
        })
        .collect::<Vec<_>>();
    (contextual.len() == 1).then(|| contextual[0])
}

pub fn list_versions(db: &DbState, document_id: &str) -> AppResult<Vec<DocumentVersion>> {
    with_conn(db, |conn| {
        let mut statement = conn.prepare(
            "SELECT id, document_id, sha256, legacy_hash, file_size, path, title,
                    change_kind, observed_at
             FROM document_versions WHERE document_id = ?1
             ORDER BY observed_at DESC, rowid DESC",
        )?;
        let rows = statement.query_map(params![document_id], |row| {
            Ok(DocumentVersion {
                id: row.get(0)?,
                document_id: row.get(1)?,
                sha256: row.get(2)?,
                legacy_hash: row.get(3)?,
                file_size: row.get(4)?,
                path: row.get(5)?,
                title: row.get(6)?,
                change_kind: row.get(7)?,
                observed_at: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    })
}

pub fn remove_from_library(db: &DbState, id: &str) -> AppResult<()> {
    with_conn(db, |conn| {
        let changed = conn.execute("DELETE FROM documents WHERE id = ?1", params![id])?;
        if changed == 0 {
            return Err(AppError::Message("document not found".into()));
        }
        Ok(())
    })
}

pub fn get_document(db: &DbState, id: &str) -> AppResult<DocumentRecord> {
    with_conn(db, |conn| {
        conn.query_row(
            "SELECT d.id, d.content_hash, d.sha256, d.title, d.last_path, d.doc_type,
                    d.file_size, d.added_at, d.last_opened_at,
                    (SELECT COUNT(*) FROM document_versions v WHERE v.document_id = d.id)
             FROM documents d WHERE d.id = ?1",
            params![id],
            map_document,
        )
        .map_err(|_| AppError::Message("document not found".into()))
    })
}

pub fn reopen_by_id(db: &DbState, id: &str) -> AppResult<OpenResult> {
    let document = get_document(db, id)?;
    let path = document
        .last_path
        .as_ref()
        .ok_or_else(|| AppError::Message("document has no path; open via dialog".into()))?;
    let path = Path::new(path);
    if !path.is_file() {
        return Err(AppError::Message(format!(
            "file missing at last path: {path}. Choose the file again via Open.",
            path = path.display()
        )));
    }
    open_and_register(db, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotations::{self, AnnotationInput};
    use std::sync::Mutex;

    struct TestDirectory(std::path::PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "soheidesk-library-test-{}",
                Uuid::new_v4().simple()
            ));
            std::fs::create_dir_all(&path).expect("test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn test_state(directory: &TestDirectory) -> DbState {
        let database = directory.0.join("soheidesk.sqlite");
        let connection = crate::db::open(&database).expect("database");
        DbState {
            conn: Mutex::new(connection),
            media: Mutex::new(()),
            data_dir: directory.0.clone(),
        }
    }

    #[test]
    fn moved_document_keeps_identity_annotations_and_history() {
        let directory = TestDirectory::new();
        let state = test_state(&directory);
        let original = directory.0.join("original.pdf");
        let moved = directory.0.join("renamed.pdf");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/sample.pdf"),
            &original,
        )
        .expect("PDF fixture");

        let first = open_and_register(&state, &original).expect("initial open");
        annotations::create(
            &state,
            AnnotationInput {
                document_id: first.document.id.clone(),
                ann_type: "highlight".into(),
                page: Some(1),
                position_json: r#"{"page":1,"rects":[{"x":1,"y":2,"w":3,"h":4}]}"#.into(),
                content: None,
                color: None,
                selected_text: None,
                context_before: None,
                context_after: None,
            },
        )
        .expect("annotation");
        std::fs::rename(&original, &moved).expect("move document");

        let reopened = open_and_register(&state, &moved).expect("open moved document");
        assert_eq!(reopened.document.id, first.document.id);
        assert!(reopened.movement_detected);
        assert!(!reopened.content_changed);
        assert_eq!(
            annotations::list_for_document(&state, &first.document.id)
                .unwrap()
                .len(),
            1
        );
        assert!(list_versions(&state, &first.document.id)
            .unwrap()
            .iter()
            .any(|version| version.change_kind == "moved"));
    }

    #[test]
    fn changed_document_rebinds_or_flags_annotations_without_deleting_them() {
        let directory = TestDirectory::new();
        let state = test_state(&directory);
        let path = directory.0.join("changing.txt");
        std::fs::write(&path, "before selected text after and obsolete").expect("document");
        let first = open_and_register(&state, &path).expect("initial open");

        for (selected_text, before, after) in [
            ("selected", "before ", " text after"),
            ("obsolete", " after and ", ""),
        ] {
            annotations::create(
                &state,
                AnnotationInput {
                    document_id: first.document.id.clone(),
                    ann_type: "highlight".into(),
                    page: None,
                    position_json: r#"{"start_offset":0,"end_offset":1}"#.into(),
                    content: None,
                    color: None,
                    selected_text: Some(selected_text.into()),
                    context_before: Some(before.into()),
                    context_after: Some(after.into()),
                },
            )
            .expect("annotation");
        }
        std::fs::write(&path, "prefix before selected text after suffix")
            .expect("updated document");

        let reopened = open_and_register(&state, &path).expect("open updated document");
        assert_eq!(reopened.document.id, first.document.id);
        assert!(reopened.content_changed);
        assert_eq!(reopened.annotations_rebound, 1);
        assert_eq!(reopened.annotations_needing_review, 1);
        let annotations = annotations::list_for_document(&state, &first.document.id).unwrap();
        assert_eq!(annotations.len(), 2);
        assert!(annotations
            .iter()
            .any(|item| item.anchor_status == "rebound"));
        assert!(annotations
            .iter()
            .any(|item| item.anchor_status == "needs_review"));
        assert!(list_versions(&state, &first.document.id)
            .unwrap()
            .iter()
            .any(|version| version.change_kind == "content_changed"));
    }

    #[test]
    fn legacy_fingerprint_is_upgraded_without_duplicate_document() {
        let directory = TestDirectory::new();
        let state = test_state(&directory);
        let path = directory.0.join("legacy.txt");
        std::fs::write(&path, "legacy document").expect("document");
        let (legacy, size) = documents::legacy_content_hash(&path).expect("legacy hash");
        let id = Uuid::new_v4().to_string();
        with_conn(&state, |connection| {
            connection.execute(
                "INSERT INTO documents (
                    id, content_hash, title, last_path, doc_type, file_size, added_at
                 ) VALUES (?1, ?2, 'Legacy', ?3, 'txt', ?4, ?5)",
                params![
                    id,
                    legacy,
                    path.to_string_lossy(),
                    size as i64,
                    Utc::now().to_rfc3339()
                ],
            )?;
            Ok(())
        })
        .expect("legacy row");

        let opened = open_and_register(&state, &path).expect("upgrade identity");
        assert_eq!(opened.document.id, id);
        assert_eq!(
            opened.document.sha256.as_deref(),
            Some(opened.opened.content_hash.as_str())
        );
        assert_eq!(list_documents(&state).unwrap().len(), 1);
    }

    #[test]
    fn ambiguous_path_and_digest_match_fails_without_reassigning_records() {
        let directory = TestDirectory::new();
        let state = test_state(&directory);
        let first_path = directory.0.join("first.txt");
        let second_path = directory.0.join("second.txt");
        std::fs::write(&first_path, "first content").expect("first document");
        std::fs::write(&second_path, "second content").expect("second document");
        let first = open_and_register(&state, &first_path).expect("open first");
        let second = open_and_register(&state, &second_path).expect("open second");

        std::fs::write(&first_path, "second content").expect("replace first content");
        let error = open_and_register(&state, &first_path).expect_err("ambiguous identity");
        assert!(error
            .to_string()
            .contains("matches another library document"));

        let documents = list_documents(&state).expect("documents after refusal");
        assert_eq!(documents.len(), 2);
        assert_eq!(
            get_document(&state, &first.document.id).unwrap().sha256,
            first.document.sha256
        );
        assert_eq!(
            get_document(&state, &second.document.id).unwrap().sha256,
            second.document.sha256
        );
    }
}
