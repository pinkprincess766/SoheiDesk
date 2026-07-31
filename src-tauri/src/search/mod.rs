use crate::db::{with_conn, DbState};
use crate::error::{AppError, AppResult};
use crate::parsers;
use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;
use tantivy::collector::TopDocs;
use tantivy::directory::MmapDirectory;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument};

pub struct SearchState {
    index: Mutex<Index>,
    reader: Mutex<IndexReader>,
    fields: SearchFields,
}

struct SearchFields {
    id: Field,
    kind: Field,
    title: Field,
    body: Field,
    path: Field,
}

#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub snippet: String,
    pub path: Option<String>,
    pub score: f32,
}

impl SearchState {
    pub fn open(data_dir: &Path) -> AppResult<Self> {
        let index_dir = data_dir.join("tantivy_index");
        std::fs::create_dir_all(&index_dir)?;

        let mut schema_builder = Schema::builder();
        let id = schema_builder.add_text_field("id", STRING | STORED);
        let kind = schema_builder.add_text_field("kind", STRING | STORED);
        let title = schema_builder.add_text_field("title", TEXT | STORED);
        let body = schema_builder.add_text_field("body", TEXT | STORED);
        let path = schema_builder.add_text_field("path", STRING | STORED);
        let schema = schema_builder.build();
        let fields = SearchFields {
            id,
            kind,
            title,
            body,
            path,
        };

        let dir = MmapDirectory::open(&index_dir)
            .map_err(|e| AppError::Message(format!("tantivy dir: {e}")))?;
        let index = Index::open_or_create(dir, schema)
            .map_err(|e| AppError::Message(format!("tantivy open_or_create: {e}")))?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| AppError::Message(format!("tantivy reader: {e}")))?;

        Ok(Self {
            index: Mutex::new(index),
            reader: Mutex::new(reader),
            fields,
        })
    }

    fn writer(&self) -> AppResult<IndexWriter> {
        let index = self
            .index
            .lock()
            .map_err(|_| AppError::Message("search index lock".into()))?;
        index
            .writer(50_000_000)
            .map_err(|e| AppError::Message(format!("tantivy writer: {e}")))
    }

    pub fn upsert_document(
        &self,
        id: &str,
        kind: &str,
        title: &str,
        body: &str,
        path: Option<&str>,
    ) -> AppResult<()> {
        let mut writer = self.writer()?;
        let term = tantivy::Term::from_field_text(self.fields.id, id);
        writer.delete_term(term);
        let mut document = TantivyDocument::default();
        document.add_text(self.fields.id, id);
        document.add_text(self.fields.kind, kind);
        document.add_text(self.fields.title, title);
        document.add_text(self.fields.body, body);
        document.add_text(self.fields.path, path.unwrap_or(""));
        writer
            .add_document(document)
            .map_err(|e| AppError::Message(format!("tantivy add: {e}")))?;
        writer
            .commit()
            .map_err(|e| AppError::Message(format!("tantivy commit: {e}")))?;
        let reader = self
            .reader
            .lock()
            .map_err(|_| AppError::Message("search reader lock".into()))?;
        reader
            .reload()
            .map_err(|e| AppError::Message(format!("tantivy reload: {e}")))?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        let mut writer = self.writer()?;
        let term = tantivy::Term::from_field_text(self.fields.id, id);
        writer.delete_term(term);
        writer
            .commit()
            .map_err(|e| AppError::Message(format!("tantivy commit: {e}")))?;
        if let Ok(reader) = self.reader.lock() {
            let _ = reader.reload();
        }
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> AppResult<Vec<SearchHit>> {
        if query.trim().is_empty() {
            return Ok(vec![]);
        }
        let index = self
            .index
            .lock()
            .map_err(|_| AppError::Message("search index lock".into()))?;
        let reader = self
            .reader
            .lock()
            .map_err(|_| AppError::Message("search reader lock".into()))?;
        let searcher = reader.searcher();
        let parser = QueryParser::for_index(&index, vec![self.fields.title, self.fields.body]);
        let q = parser
            .parse_query(query)
            .map_err(|e| AppError::Message(format!("bad query: {e}")))?;
        let top = searcher
            .search(&q, &TopDocs::with_limit(limit.clamp(1, 100)))
            .map_err(|e| AppError::Message(format!("search: {e}")))?;

        let mut hits = Vec::new();
        for (score, addr) in top {
            let retrieved: TantivyDocument = searcher
                .doc(addr)
                .map_err(|e| AppError::Message(format!("doc: {e}")))?;
            let get = |f: Field| -> String {
                retrieved
                    .get_first(f)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            let body = get(self.fields.body);
            let snippet = body.chars().take(180).collect::<String>();
            let path = get(self.fields.path);
            hits.push(SearchHit {
                id: get(self.fields.id),
                kind: get(self.fields.kind),
                title: get(self.fields.title),
                snippet,
                path: if path.is_empty() { None } else { Some(path) },
                score,
            });
        }
        Ok(hits)
    }

    pub fn reindex_all(&self, db: &DbState) -> AppResult<u64> {
        let mut writer = self.writer()?;
        writer
            .delete_all_documents()
            .map_err(|e| AppError::Message(format!("tantivy delete_all: {e}")))?;
        writer
            .commit()
            .map_err(|e| AppError::Message(format!("tantivy commit: {e}")))?;

        let mut count = 0u64;

        let docs: Vec<(String, String, Option<String>, String)> = with_conn(db, |conn| {
            let mut stmt = conn.prepare("SELECT id, title, last_path, doc_type FROM documents")?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get::<_, Option<String>>(1)?
                        .unwrap_or_else(|| "Untitled".into()),
                    row.get(2)?,
                    row.get(3)?,
                ))
            })?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r?);
            }
            Ok(v)
        })?;

        for (id, title, path, doc_type) in docs {
            let body = if let Some(ref p) = path {
                parsers::extract_search_text(Path::new(p), &doc_type).unwrap_or_default()
            } else {
                String::new()
            };
            self.upsert_document(&id, "document", &title, &body, path.as_deref())?;
            count += 1;
        }

        let entries: Vec<(String, String, String)> = with_conn(db, |conn| {
            let mut stmt = conn.prepare("SELECT id, title, body_md FROM journal_entries")?;
            let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
            let mut v = Vec::new();
            for r in rows {
                v.push(r?);
            }
            Ok(v)
        })?;

        for (id, title, body) in entries {
            self.upsert_document(&id, "journal", &title, &body, None)?;
            count += 1;
        }

        Ok(count)
    }
}

pub fn index_opened_document(
    search: &SearchState,
    id: &str,
    title: &str,
    path: &str,
    doc_type: &str,
    text: Option<&str>,
) -> AppResult<()> {
    let body = if let Some(t) = text {
        t.to_string()
    } else {
        parsers::extract_search_text(Path::new(path), doc_type).unwrap_or_default()
    };
    search.upsert_document(id, "document", title, &body, Some(path))
}

pub fn index_journal_entry(
    search: &SearchState,
    id: &str,
    title: &str,
    body: &str,
) -> AppResult<()> {
    search.upsert_document(id, "journal", title, body, None)
}
