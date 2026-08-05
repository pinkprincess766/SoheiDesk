//! Portable, standard-ZIP workspace packages with preview-gated import.

use crate::atomic_file;
use crate::backup::{self, BackupInfo, BackupState};
use crate::db::{self, DbState};
use crate::error::{AppError, AppResult};
use crate::search::SearchState;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const WORKSPACE_FORMAT_VERSION: u32 = 1;
const PACKAGE_TYPE: &str = "soheidesk-workspace";
const ROOT: &str = "soheidesk-backup";
const MANIFEST_PATH: &str = "manifest.json";
const DATABASE_PATH: &str = "database.sqlite";
const README_PATH: &str = "README.txt";
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
const MAX_README_BYTES: u64 = 1024 * 1024;
const MAX_PACKAGE_BYTES: u64 = 50 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;
const PREVIEW_LIFETIME: Duration = Duration::from_secs(30 * 60);

const PACKAGE_README: &str = r#"SoheiDesk portable workspace
============================

This is a standard ZIP archive. You do not need SoheiDesk to read it.

database.sqlite  Complete workspace database in the SQLite format.
attachments/     Copies of source documents and journal file fields that were available during export.
media/           Images and other media extracted or cached by SoheiDesk.
manifest.json    Format, compatibility, file sizes, SHA-256 checksums, and path mappings.

You can inspect database.sqlite with sqlite3, DB Browser for SQLite, or another
SQLite-compatible tool. Text fields and annotations are stored in ordinary
tables. Missing external files, if any, are listed in manifest.json.

The archive is integrity checked but is not encrypted. Protect it like the
original research data.
"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    Document,
    JournalField,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttachmentReference {
    pub kind: AttachmentKind,
    pub record_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field_key: Option<String>,
    pub original_path: String,
    pub archive_path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceCounts {
    pub settings: u64,
    pub documents: u64,
    pub annotations: u64,
    pub journal_entries: u64,
    pub journal_drafts: u64,
    pub user_templates: u64,
    pub user_export_templates: u64,
    pub bibliography_items: u64,
    pub rss_feeds: u64,
    pub rss_items: u64,
    pub plugins: u64,
}

impl WorkspaceCounts {
    pub fn total_records(&self) -> u64 {
        [
            self.settings,
            self.documents,
            self.annotations,
            self.journal_entries,
            self.journal_drafts,
            self.user_templates,
            self.user_export_templates,
            self.bibliography_items,
            self.rss_feeds,
            self.rss_items,
            self.plugins,
        ]
        .into_iter()
        .fold(0, u64::saturating_add)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    pub format_version: u32,
    pub package_type: String,
    pub id: String,
    pub created_at: String,
    pub app_version: String,
    pub schema_version: i64,
    pub counts: WorkspaceCounts,
    pub files: Vec<WorkspaceFile>,
    pub attachment_references: Vec<AttachmentReference>,
    pub missing_references: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceExportResult {
    pub path: String,
    pub created_at: String,
    pub schema_version: i64,
    pub counts: WorkspaceCounts,
    pub file_count: u64,
    pub total_size: u64,
    pub attachment_count: u64,
    pub media_count: u64,
    pub missing_references: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspacePreview {
    pub token: String,
    pub file_name: String,
    pub created_at: String,
    pub app_version: String,
    pub schema_version: i64,
    pub compatibility: String,
    pub counts: WorkspaceCounts,
    pub current_counts: WorkspaceCounts,
    pub file_count: u64,
    pub total_size: u64,
    pub attachment_count: u64,
    pub media_count: u64,
    pub missing_references: Vec<String>,
    pub requires_replacement_confirmation: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceImportResult {
    pub imported_counts: WorkspaceCounts,
    pub emergency: BackupInfo,
    pub reindexed_items: Option<u64>,
    pub warning: Option<String>,
}

#[derive(Clone)]
struct ImportAuthorization {
    token: String,
    source: PathBuf,
    archive_sha256: String,
    current_database_sha256: String,
    requires_replacement: bool,
    created_at: Instant,
}

#[derive(Default)]
pub struct PortabilityState {
    operation: Mutex<()>,
    preview: Mutex<Option<ImportAuthorization>>,
}

struct PayloadFile {
    source: PathBuf,
    path: String,
    size: u64,
    sha256: String,
}

struct AttachmentCandidate {
    kind: AttachmentKind,
    record_id: String,
    field_key: Option<String>,
    original_path: String,
}

struct ExtractedWorkspace {
    _staging: TempDir,
    database: PathBuf,
    media: PathBuf,
    attachments: PathBuf,
    manifest: WorkspaceManifest,
}

struct TempDir(PathBuf);

impl TempDir {
    fn create(parent: &Path, prefix: &str) -> AppResult<Self> {
        fs::create_dir_all(parent)?;
        let path = parent.join(format!(".{prefix}-{}", Uuid::new_v4().simple()));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path)?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn zip_error(context: &str, error: zip::result::ZipError) -> AppError {
    AppError::Message(format!("{context}: {error}"))
}

fn count_query(conn: &Connection, sql: &str) -> AppResult<u64> {
    let count: i64 = conn.query_row(sql, [], |row| row.get(0))?;
    u64::try_from(count).map_err(|_| AppError::Message("negative record count".into()))
}

fn table_exists(conn: &Connection, table: &str) -> AppResult<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |row| row.get(0),
    )?)
}

fn count_if_table_exists(conn: &Connection, table: &str, sql: &str) -> AppResult<u64> {
    if table_exists(conn, table)? {
        count_query(conn, sql)
    } else {
        Ok(0)
    }
}

fn workspace_counts(conn: &Connection) -> AppResult<WorkspaceCounts> {
    Ok(WorkspaceCounts {
        settings: count_query(conn, "SELECT COUNT(*) FROM settings")?,
        documents: count_query(conn, "SELECT COUNT(*) FROM documents")?,
        annotations: count_query(conn, "SELECT COUNT(*) FROM annotations")?,
        journal_entries: count_query(conn, "SELECT COUNT(*) FROM journal_entries")?,
        journal_drafts: count_if_table_exists(
            conn,
            "journal_drafts",
            "SELECT COUNT(*) FROM journal_drafts",
        )?,
        user_templates: count_query(conn, "SELECT COUNT(*) FROM templates WHERE is_builtin=0")?,
        user_export_templates: count_if_table_exists(
            conn,
            "export_templates",
            "SELECT COUNT(*) FROM export_templates WHERE is_builtin=0",
        )?,
        bibliography_items: count_if_table_exists(
            conn,
            "bibliography_items",
            "SELECT COUNT(*) FROM bibliography_items",
        )?,
        rss_feeds: count_if_table_exists(conn, "rss_feeds", "SELECT COUNT(*) FROM rss_feeds")?,
        rss_items: count_if_table_exists(conn, "rss_items", "SELECT COUNT(*) FROM rss_items")?,
        plugins: count_if_table_exists(conn, "plugins", "SELECT COUNT(*) FROM plugins")?,
    })
}

fn payload_file(source: PathBuf, path: impl Into<String>) -> AppResult<PayloadFile> {
    let (size, sha256) = backup::hash_file(&source)?;
    Ok(PayloadFile {
        source,
        path: path.into(),
        size,
        sha256,
    })
}

fn collect_tree_files(
    current: &Path,
    root: &Path,
    archive_prefix: &str,
    payload: &mut Vec<PayloadFile>,
) -> AppResult<()> {
    if !current.exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(current)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(AppError::Message(format!(
                "workspace export refuses symlink: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            collect_tree_files(&path, root, archive_prefix, payload)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(AppError::Message(format!(
                "unsupported workspace file: {}",
                path.display()
            )));
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| AppError::Message("invalid workspace tree path".into()))?
            .to_str()
            .ok_or_else(|| AppError::Message("workspace filename is not valid UTF-8".into()))?
            .replace('\\', "/");
        payload.push(payload_file(path, format!("{archive_prefix}/{relative}"))?);
    }
    Ok(())
}

fn file_keys(fields: &serde_json::Value) -> HashSet<String> {
    fields
        .as_array()
        .into_iter()
        .flatten()
        .filter(|field| field.get("type").and_then(|value| value.as_str()) == Some("file"))
        .filter_map(|field| field.get("key").and_then(|value| value.as_str()))
        .map(str::to_string)
        .collect()
}

fn collect_attachment_candidates(conn: &Connection) -> AppResult<Vec<AttachmentCandidate>> {
    let mut candidates = Vec::new();
    let mut documents = conn
        .prepare("SELECT id, last_path FROM documents WHERE last_path IS NOT NULL ORDER BY id")?;
    for row in documents.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })? {
        let (record_id, original_path) = row?;
        candidates.push(AttachmentCandidate {
            kind: AttachmentKind::Document,
            record_id,
            field_key: None,
            original_path,
        });
    }

    let mut template_keys = HashMap::new();
    let mut templates = conn.prepare("SELECT id, fields_json FROM templates")?;
    for row in templates.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })? {
        let (id, fields_json) = row?;
        let fields = serde_json::from_str(&fields_json).unwrap_or(serde_json::Value::Null);
        template_keys.insert(id, file_keys(&fields));
    }

    let mut entries = conn.prepare(
        "SELECT id, fields_json, template_snapshot_json, template_id
         FROM journal_entries WHERE fields_json IS NOT NULL ORDER BY id",
    )?;
    let rows = entries.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
        ))
    })?;
    for row in rows {
        let (record_id, fields_json, snapshot_json, template_id) = row?;
        let keys = snapshot_json
            .as_deref()
            .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
            .and_then(|value| value.get("fields").cloned())
            .map(|fields| file_keys(&fields))
            .or_else(|| {
                template_id
                    .as_ref()
                    .and_then(|id| template_keys.get(id).cloned())
            })
            .unwrap_or_default();
        let fields: serde_json::Value = serde_json::from_str(&fields_json)?;
        for key in keys {
            if let Some(original_path) = fields.get(&key).and_then(|value| value.as_str()) {
                if !original_path.trim().is_empty() {
                    candidates.push(AttachmentCandidate {
                        kind: AttachmentKind::JournalField,
                        record_id: record_id.clone(),
                        field_key: Some(key),
                        original_path: original_path.into(),
                    });
                }
            }
        }
    }
    Ok(candidates)
}

fn safe_file_name(path: &Path) -> String {
    let raw = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("attachment");
    let mut safe = raw
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if safe.is_empty() || safe == "." || safe == ".." {
        safe = "attachment".into();
    }
    safe.truncate(96);
    safe
}

fn stage_attachments(
    conn: &Connection,
    staging: &Path,
    payload: &mut Vec<PayloadFile>,
) -> AppResult<(Vec<AttachmentReference>, Vec<String>)> {
    let target = staging.join("attachments");
    fs::create_dir_all(&target)?;
    let mut staged = HashMap::<PathBuf, String>::new();
    let mut references = Vec::new();
    let mut missing = Vec::new();

    for candidate in collect_attachment_candidates(conn)? {
        let original = Path::new(&candidate.original_path);
        let metadata = match fs::symlink_metadata(original) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(format!(
                    "{:?} {}: {}",
                    candidate.kind, candidate.record_id, candidate.original_path
                ));
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            missing.push(format!(
                "{:?} {}: {} (not a regular file)",
                candidate.kind, candidate.record_id, candidate.original_path
            ));
            continue;
        }
        let canonical = fs::canonicalize(original)?;
        let archive_path = if let Some(existing) = staged.get(&canonical) {
            existing.clone()
        } else {
            let temporary = target.join(format!(".copy-{}", Uuid::new_v4().simple()));
            fs::copy(&canonical, &temporary)?;
            File::open(&temporary)?.sync_all()?;
            let (_, file_sha256) = backup::hash_file(&temporary)?;
            let path_sha256 = hex::encode(Sha256::digest(canonical.to_string_lossy().as_bytes()));
            let name = format!(
                "{}-{}-{}",
                &file_sha256[..16],
                &path_sha256[..8],
                safe_file_name(&canonical)
            );
            let relative = format!("attachments/{name}");
            let final_path = target.join(&name);
            fs::rename(&temporary, &final_path)?;
            payload.push(payload_file(final_path, &relative)?);
            staged.insert(canonical, relative.clone());
            relative
        };
        references.push(AttachmentReference {
            kind: candidate.kind,
            record_id: candidate.record_id,
            field_key: candidate.field_key,
            original_path: candidate.original_path,
            archive_path,
        });
    }
    Ok((references, missing))
}

fn validate_relative_path(path: &str) -> AppResult<()> {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') {
        return Err(AppError::Message(format!("unsafe workspace path: {path}")));
    }
    for component in Path::new(path).components() {
        let Component::Normal(name) = component else {
            return Err(AppError::Message(format!("unsafe workspace path: {path}")));
        };
        let name = name
            .to_str()
            .ok_or_else(|| AppError::Message(format!("unsafe workspace path: {path}")))?;
        let stem = name
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if name.contains(':')
            || name.chars().any(char::is_control)
            || name.ends_with([' ', '.'])
            || matches!(
                stem.as_str(),
                "CON"
                    | "PRN"
                    | "AUX"
                    | "NUL"
                    | "COM1"
                    | "COM2"
                    | "COM3"
                    | "COM4"
                    | "COM5"
                    | "COM6"
                    | "COM7"
                    | "COM8"
                    | "COM9"
                    | "LPT1"
                    | "LPT2"
                    | "LPT3"
                    | "LPT4"
                    | "LPT5"
                    | "LPT6"
                    | "LPT7"
                    | "LPT8"
                    | "LPT9"
            )
        {
            return Err(AppError::Message(format!("unsafe workspace path: {path}")));
        }
    }
    Ok(())
}

fn validate_manifest(manifest: &WorkspaceManifest) -> AppResult<()> {
    if manifest.format_version != WORKSPACE_FORMAT_VERSION {
        return Err(AppError::Message(format!(
            "workspace format {} is not supported by this app (supported: {})",
            manifest.format_version, WORKSPACE_FORMAT_VERSION
        )));
    }
    if manifest.package_type != PACKAGE_TYPE || Uuid::parse_str(&manifest.id).is_err() {
        return Err(AppError::Message(
            "invalid workspace package identity".into(),
        ));
    }
    DateTime::parse_from_rfc3339(&manifest.created_at)
        .map_err(|error| AppError::Message(format!("invalid workspace timestamp: {error}")))?;
    if manifest.schema_version > db::migrations::latest_version() {
        return Err(AppError::Message(format!(
            "workspace requires database schema {} but this app supports up to {}; update SoheiDesk before importing",
            manifest.schema_version,
            db::migrations::latest_version()
        )));
    }
    if manifest.schema_version < 1 {
        return Err(AppError::Message("invalid workspace schema version".into()));
    }
    if manifest.files.len() > MAX_ENTRIES
        || manifest.attachment_references.len() > MAX_ENTRIES
        || manifest.missing_references.len() > MAX_ENTRIES
    {
        return Err(AppError::Message(
            "workspace manifest has too many entries".into(),
        ));
    }

    let mut paths = HashSet::new();
    let mut portable_paths = HashSet::new();
    let mut total = 0_u64;
    for file in &manifest.files {
        validate_relative_path(&file.path)?;
        let allowed = file.path == DATABASE_PATH
            || file.path == README_PATH
            || file.path.starts_with("attachments/")
            || file.path.starts_with("media/");
        if !allowed || !paths.insert(file.path.as_str()) {
            return Err(AppError::Message(format!(
                "invalid or duplicate workspace payload: {}",
                file.path
            )));
        }
        if !portable_paths.insert(file.path.to_ascii_lowercase()) {
            return Err(AppError::Message(format!(
                "case-insensitive workspace path collision: {}",
                file.path
            )));
        }
        if file.sha256.len() != 64 || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(AppError::Message(format!(
                "invalid checksum for {}",
                file.path
            )));
        }
        total = total
            .checked_add(file.size)
            .ok_or_else(|| AppError::Message("workspace package is too large".into()))?;
    }
    for required in [DATABASE_PATH, README_PATH] {
        if !paths.contains(required) {
            return Err(AppError::Message(format!(
                "workspace is missing required file: {required}"
            )));
        }
    }
    if total > MAX_PACKAGE_BYTES {
        return Err(AppError::Message(
            "workspace package exceeds safety limit".into(),
        ));
    }
    let mut reference_targets = HashSet::new();
    for reference in &manifest.attachment_references {
        let target = (
            reference.kind.clone(),
            reference.record_id.as_str(),
            reference.field_key.as_deref(),
        );
        if reference.record_id.is_empty()
            || reference.original_path.is_empty()
            || !reference.archive_path.starts_with("attachments/")
            || !paths.contains(reference.archive_path.as_str())
            || (reference.kind == AttachmentKind::JournalField
                && reference
                    .field_key
                    .as_deref()
                    .unwrap_or_default()
                    .is_empty())
            || (reference.kind == AttachmentKind::Document && reference.field_key.is_some())
            || !reference_targets.insert(target)
        {
            return Err(AppError::Message(
                "invalid attachment reference in workspace manifest".into(),
            ));
        }
    }
    Ok(())
}

fn archive_name(relative: &str) -> String {
    format!("{ROOT}/{relative}")
}

fn write_archive(
    file: File,
    payload: &[PayloadFile],
    manifest: &WorkspaceManifest,
) -> AppResult<File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    let mut zip = ZipWriter::new(file);
    let file_options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o600);
    let directory_options = SimpleFileOptions::default().unix_permissions(0o700);
    for directory in [
        format!("{ROOT}/"),
        archive_name("attachments/"),
        archive_name("media/"),
    ] {
        zip.add_directory(directory, directory_options)
            .map_err(|error| zip_error("add workspace directory", error))?;
    }
    let mut buffer = [0_u8; 64 * 1024];
    for item in payload {
        zip.start_file(archive_name(&item.path), file_options)
            .map_err(|error| zip_error("start workspace payload", error))?;
        let mut source = File::open(&item.source)?;
        loop {
            let read = source.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            zip.write_all(&buffer[..read])?;
        }
    }
    zip.start_file(archive_name(MANIFEST_PATH), file_options)
        .map_err(|error| zip_error("start workspace manifest", error))?;
    zip.write_all(&serde_json::to_vec_pretty(manifest)?)?;
    zip.finish()
        .map_err(|error| zip_error("finish workspace archive", error))
}

fn read_manifest(path: &Path) -> AppResult<WorkspaceManifest> {
    let mut archive = ZipArchive::new(File::open(path)?)
        .map_err(|error| zip_error("open workspace archive", error))?;
    if archive.len() > MAX_ENTRIES + 4 {
        return Err(AppError::Message(
            "workspace contains too many entries".into(),
        ));
    }
    let manifest_name = archive_name(MANIFEST_PATH);
    let count = (0..archive.len())
        .filter(|index| {
            archive
                .by_index(*index)
                .map(|entry| entry.name() == manifest_name)
                .unwrap_or(false)
        })
        .count();
    if count != 1 {
        return Err(AppError::Message(
            "workspace must contain exactly one manifest".into(),
        ));
    }
    let mut entry = archive
        .by_name(&manifest_name)
        .map_err(|error| zip_error("read workspace manifest", error))?;
    if entry.size() > MAX_MANIFEST_BYTES {
        return Err(AppError::Message("workspace manifest is too large".into()));
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut bytes)?;
    let manifest: WorkspaceManifest = serde_json::from_slice(&bytes)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_and_extract(path: &Path) -> AppResult<ExtractedWorkspace> {
    let staging_parent = path
        .parent()
        .ok_or_else(|| AppError::Message("workspace archive has no parent directory".into()))?;
    validate_and_extract_in(path, staging_parent)
}

fn validate_and_extract_in(path: &Path, staging_parent: &Path) -> AppResult<ExtractedWorkspace> {
    let manifest = read_manifest(path)?;
    // Import callers choose app data as the staging parent so directory swaps
    // remain atomic even when the source archive is on another volume.
    let staging = TempDir::create(staging_parent, "workspace-import")?;
    let root = staging.path().join(ROOT);
    fs::create_dir_all(root.join("attachments"))?;
    fs::create_dir_all(root.join("media"))?;

    let mut archive = ZipArchive::new(File::open(path)?)
        .map_err(|error| zip_error("open workspace archive", error))?;
    let allowed_directories = [
        format!("{ROOT}/"),
        archive_name("attachments/"),
        archive_name("media/"),
    ]
    .into_iter()
    .collect::<HashSet<_>>();
    let expected = manifest
        .files
        .iter()
        .map(|file| archive_name(&file.path))
        .chain(std::iter::once(archive_name(MANIFEST_PATH)))
        .collect::<HashSet<_>>();
    if archive.len() != expected.len() + allowed_directories.len() {
        return Err(AppError::Message(
            "workspace contents do not match manifest".into(),
        ));
    }
    let mut names = HashSet::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| zip_error("read workspace entry", error))?;
        let name = entry.name().to_string();
        if allowed_directories.contains(&name) && (!entry.is_dir() || entry.size() != 0) {
            return Err(AppError::Message(format!(
                "invalid workspace directory entry: {name}"
            )));
        }
        if !names.insert(name.clone())
            || (!expected.contains(&name) && !allowed_directories.contains(&name))
        {
            return Err(AppError::Message(format!(
                "unknown or duplicate workspace entry: {name}"
            )));
        }
    }

    for declared in &manifest.files {
        let archive_path = archive_name(&declared.path);
        let mut source = archive
            .by_name(&archive_path)
            .map_err(|error| zip_error("read workspace payload", error))?;
        if source.size() != declared.size || source.size() > MAX_PACKAGE_BYTES {
            return Err(AppError::Message(format!(
                "size mismatch for {}",
                declared.path
            )));
        }
        let destination = root.join(&declared.path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = File::create(destination)?;
        let mut hasher = Sha256::new();
        let mut written = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = source.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            written = written
                .checked_add(read as u64)
                .ok_or_else(|| AppError::Message("workspace payload is too large".into()))?;
            if written > declared.size {
                return Err(AppError::Message(format!(
                    "expanded size mismatch for {}",
                    declared.path
                )));
            }
            output.write_all(&buffer[..read])?;
            hasher.update(&buffer[..read]);
        }
        output.sync_all()?;
        if written != declared.size || hex::encode(hasher.finalize()) != declared.sha256 {
            return Err(AppError::Message(format!(
                "checksum mismatch for {}",
                declared.path
            )));
        }
    }

    let database = root.join(DATABASE_PATH);
    let connection = Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    backup::verify_database(&connection)?;
    let schema_version = db::migrations::current_version(&connection)?;
    if schema_version != manifest.schema_version {
        return Err(AppError::Message(format!(
            "workspace database schema {schema_version} does not match manifest {}",
            manifest.schema_version
        )));
    }
    let counts = workspace_counts(&connection)?;
    if counts != manifest.counts {
        return Err(AppError::Message(
            "workspace record counts do not match the database".into(),
        ));
    }
    drop(connection);
    let readme = fs::read(root.join(README_PATH))?;
    if readme.len() as u64 > MAX_README_BYTES || std::str::from_utf8(&readme).is_err() {
        return Err(AppError::Message("workspace README is invalid".into()));
    }

    Ok(ExtractedWorkspace {
        database,
        media: root.join("media"),
        attachments: root.join("attachments"),
        _staging: staging,
        manifest,
    })
}

fn database_snapshot_sha256(conn: &Connection, parent: &Path) -> AppResult<String> {
    let staging = TempDir::create(parent, "workspace-current")?;
    let snapshot = staging.path().join("current.sqlite");
    backup::create_sqlite_snapshot(conn, &snapshot)?;
    let (_, sha256) = backup::hash_file(&snapshot)?;
    Ok(sha256)
}

pub fn export_workspace(
    db: &DbState,
    state: &PortabilityState,
    destination: &Path,
) -> AppResult<WorkspaceExportResult> {
    let _operation = state
        .operation
        .lock()
        .map_err(|_| AppError::Message("workspace operation lock poisoned".into()))?;
    if destination.exists() {
        return Err(AppError::Message(
            "export destination already exists; choose a new file name".into(),
        ));
    }
    let parent = destination
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let staging = TempDir::create(&db.data_dir, "workspace-export")?;
    let snapshot = staging.path().join(DATABASE_PATH);
    let readme = staging.path().join(README_PATH);
    fs::write(&readme, PACKAGE_README.as_bytes())?;

    let _media = db
        .media
        .lock()
        .map_err(|_| AppError::Message("media lock poisoned".into()))?;
    let conn = db
        .conn
        .lock()
        .map_err(|_| AppError::Message("database lock poisoned".into()))?;
    backup::create_sqlite_snapshot(&conn, &snapshot)?;
    let snapshot_conn = Connection::open_with_flags(&snapshot, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let schema_version = db::migrations::current_version(&snapshot_conn)?;
    let counts = workspace_counts(&snapshot_conn)?;
    let mut payload = vec![
        payload_file(snapshot, DATABASE_PATH)?,
        payload_file(readme, README_PATH)?,
    ];
    let (attachment_references, missing_references) =
        stage_attachments(&snapshot_conn, staging.path(), &mut payload)?;
    drop(snapshot_conn);
    collect_tree_files(
        &db.data_dir.join("media"),
        &db.data_dir.join("media"),
        "media",
        &mut payload,
    )?;
    drop(conn);
    payload.sort_by(|left, right| left.path.cmp(&right.path));

    let total_size = payload.iter().try_fold(0_u64, |total, file| {
        total
            .checked_add(file.size)
            .ok_or_else(|| AppError::Message("workspace package is too large".into()))
    })?;
    if total_size > MAX_PACKAGE_BYTES {
        return Err(AppError::Message(
            "workspace package exceeds the 50 GiB safety limit".into(),
        ));
    }
    let created_at = Utc::now().to_rfc3339();
    let manifest = WorkspaceManifest {
        format_version: WORKSPACE_FORMAT_VERSION,
        package_type: PACKAGE_TYPE.into(),
        id: Uuid::new_v4().to_string(),
        created_at: created_at.clone(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        schema_version,
        counts: counts.clone(),
        files: payload
            .iter()
            .map(|file| WorkspaceFile {
                path: file.path.clone(),
                size: file.size,
                sha256: file.sha256.clone(),
            })
            .collect(),
        attachment_references,
        missing_references: missing_references.clone(),
    };
    validate_manifest(&manifest)?;
    atomic_file::write_file(
        destination,
        |file| write_archive(file, &payload, &manifest),
        |temporary| {
            let _verified = validate_and_extract(temporary)?;
            Ok(())
        },
    )?;
    let attachment_count = manifest
        .files
        .iter()
        .filter(|file| file.path.starts_with("attachments/"))
        .count() as u64;
    let media_count = manifest
        .files
        .iter()
        .filter(|file| file.path.starts_with("media/"))
        .count() as u64;
    Ok(WorkspaceExportResult {
        path: destination.to_string_lossy().into(),
        created_at,
        schema_version,
        counts,
        file_count: manifest.files.len() as u64,
        total_size,
        attachment_count,
        media_count,
        missing_references,
    })
}

pub fn preview_import(
    db: &DbState,
    state: &PortabilityState,
    source: &Path,
) -> AppResult<WorkspacePreview> {
    let _operation = state
        .operation
        .lock()
        .map_err(|_| AppError::Message("workspace operation lock poisoned".into()))?;
    let source = fs::canonicalize(source)?;
    if !source.is_file() || fs::metadata(&source)?.len() > MAX_PACKAGE_BYTES {
        return Err(AppError::Message(
            "invalid or oversized workspace archive".into(),
        ));
    }
    // Preview must also work for read-only or removable source volumes.
    let extracted = validate_and_extract_in(&source, &db.data_dir)?;
    let (_, archive_sha256) = backup::hash_file(&source)?;
    let conn = db
        .conn
        .lock()
        .map_err(|_| AppError::Message("database lock poisoned".into()))?;
    let current_counts = workspace_counts(&conn)?;
    let current_database_sha256 = database_snapshot_sha256(&conn, &db.data_dir)?;
    drop(conn);
    let requires_replacement = current_counts.total_records() > 0;
    let token = Uuid::new_v4().to_string();
    let authorization = ImportAuthorization {
        token: token.clone(),
        source: source.clone(),
        archive_sha256,
        current_database_sha256,
        requires_replacement,
        created_at: Instant::now(),
    };
    *state
        .preview
        .lock()
        .map_err(|_| AppError::Message("workspace preview lock poisoned".into()))? =
        Some(authorization);

    let total_size = extracted.manifest.files.iter().map(|file| file.size).sum();
    let attachment_count = extracted
        .manifest
        .files
        .iter()
        .filter(|file| file.path.starts_with("attachments/"))
        .count() as u64;
    let media_count = extracted
        .manifest
        .files
        .iter()
        .filter(|file| file.path.starts_with("media/"))
        .count() as u64;
    Ok(WorkspacePreview {
        token,
        file_name: source
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("workspace.zip")
            .into(),
        created_at: extracted.manifest.created_at.clone(),
        app_version: extracted.manifest.app_version.clone(),
        schema_version: extracted.manifest.schema_version,
        compatibility: if extracted.manifest.schema_version == db::migrations::latest_version() {
            "compatible".into()
        } else {
            "upgrade_required".into()
        },
        counts: extracted.manifest.counts.clone(),
        current_counts,
        file_count: extracted.manifest.files.len() as u64,
        total_size,
        attachment_count,
        media_count,
        missing_references: extracted.manifest.missing_references.clone(),
        requires_replacement_confirmation: requires_replacement,
    })
}

fn prepare_import_database(
    database: &Path,
    data_dir: &Path,
    references: &[AttachmentReference],
) -> AppResult<()> {
    let mut conn = Connection::open(database)?;
    db::migrations::validate_compatible(&conn)?;
    let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    for reference in references {
        let relative = reference
            .archive_path
            .strip_prefix("attachments/")
            .ok_or_else(|| AppError::Message("invalid imported attachment path".into()))?;
        let destination = data_dir.join("attachments").join(relative);
        let destination = destination.to_string_lossy().to_string();
        match reference.kind {
            AttachmentKind::Document => {
                let changed = transaction.execute(
                    "UPDATE documents SET last_path=?1 WHERE id=?2 AND last_path=?3",
                    params![destination, reference.record_id, reference.original_path],
                )?;
                if changed != 1 {
                    return Err(AppError::Message(format!(
                        "document attachment mapping does not match record {}",
                        reference.record_id
                    )));
                }
            }
            AttachmentKind::JournalField => {
                let fields_json: String = transaction
                    .query_row(
                        "SELECT fields_json FROM journal_entries WHERE id=?1",
                        params![reference.record_id],
                        |row| row.get(0),
                    )
                    .optional()?
                    .ok_or_else(|| {
                        AppError::Message("journal attachment record is missing".into())
                    })?;
                let mut fields: serde_json::Value = serde_json::from_str(&fields_json)?;
                let key = reference
                    .field_key
                    .as_deref()
                    .ok_or_else(|| AppError::Message("journal attachment key is missing".into()))?;
                let current = fields
                    .get(key)
                    .and_then(|value| value.as_str())
                    .ok_or_else(|| {
                        AppError::Message("journal attachment field is missing".into())
                    })?;
                if current != reference.original_path {
                    return Err(AppError::Message(
                        "journal attachment mapping does not match database".into(),
                    ));
                }
                fields[key] = serde_json::Value::String(destination);
                transaction.execute(
                    "UPDATE journal_entries SET fields_json=?1 WHERE id=?2",
                    params![serde_json::to_string(&fields)?, reference.record_id],
                )?;
            }
        }
    }
    transaction.commit()?;
    backup::verify_database(&conn)
}

pub fn import_workspace(
    db: &DbState,
    backups: &BackupState,
    state: &PortabilityState,
    search: &SearchState,
    token: &str,
    replace_existing: bool,
) -> AppResult<WorkspaceImportResult> {
    let _operation = state
        .operation
        .lock()
        .map_err(|_| AppError::Message("workspace operation lock poisoned".into()))?;
    let authorization = state
        .preview
        .lock()
        .map_err(|_| AppError::Message("workspace preview lock poisoned".into()))?
        .clone()
        .ok_or_else(|| AppError::Message("import preview is required".into()))?;
    if authorization.token != token || authorization.created_at.elapsed() > PREVIEW_LIFETIME {
        return Err(AppError::Message(
            "import preview is invalid or expired; preview the archive again".into(),
        ));
    }
    if authorization.requires_replacement && !replace_existing {
        return Err(AppError::Message(
            "current workspace is not empty; explicit replacement confirmation is required".into(),
        ));
    }
    let (_, current_archive_sha256) = backup::hash_file(&authorization.source)?;
    if current_archive_sha256 != authorization.archive_sha256 {
        *state
            .preview
            .lock()
            .map_err(|_| AppError::Message("workspace preview lock poisoned".into()))? = None;
        return Err(AppError::Message(
            "workspace archive changed after preview; preview it again".into(),
        ));
    }
    let extracted = validate_and_extract_in(&authorization.source, &db.data_dir)?;
    let (_, post_extract_sha256) = backup::hash_file(&authorization.source)?;
    if post_extract_sha256 != authorization.archive_sha256 {
        *state
            .preview
            .lock()
            .map_err(|_| AppError::Message("workspace preview lock poisoned".into()))? = None;
        return Err(AppError::Message(
            "workspace archive changed during import validation; preview it again".into(),
        ));
    }
    prepare_import_database(
        &extracted.database,
        &db.data_dir,
        &extracted.manifest.attachment_references,
    )?;
    *state
        .preview
        .lock()
        .map_err(|_| AppError::Message("workspace preview lock poisoned".into()))? = None;
    let outcome = backup::restore_external_snapshot(
        db,
        backups,
        search,
        &extracted.database,
        &extracted.media,
        &extracted.attachments,
        &authorization.current_database_sha256,
    )?;
    Ok(WorkspaceImportResult {
        imported_counts: extracted.manifest.counts,
        emergency: outcome.emergency,
        reindexed_items: outcome.reindexed_items,
        warning: outcome.warning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "soheidesk-portability-{name}-{}",
                Uuid::new_v4().simple()
            ));
            fs::create_dir_all(&path).expect("test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn state(directory: &TestDirectory) -> DbState {
        DbState {
            conn: Mutex::new(db::open(&directory.0.join("soheidesk.sqlite")).expect("database")),
            media: Mutex::new(()),
            data_dir: directory.0.clone(),
        }
    }

    fn add_source_data(db: &DbState, external: &Path) {
        let conn = db.conn.lock().expect("database lock");
        conn.execute(
            "INSERT INTO settings(key, value) VALUES ('portable_marker', 'source')",
            [],
        )
        .expect("setting");
        conn.execute(
            "INSERT INTO documents(
                id, content_hash, title, last_path, doc_type, file_size, added_at
             ) VALUES ('doc', 'hash', 'Document', ?1, 'txt', 10, 'now')",
            params![external.to_string_lossy()],
        )
        .expect("document");
        conn.execute(
            r#"INSERT INTO templates(
                id, name, is_builtin, fields_json, body_md, created_at, updated_at
             ) VALUES ('files', 'Files', 0, '[{"key":"data","label":"Data","type":"file"}]', '', 'now', 'now')"#,
            [],
        )
        .expect("template");
        let fields = serde_json::json!({"data": external.to_string_lossy()}).to_string();
        conn.execute(
            "INSERT INTO journal_entries(
                id, title, template_id, body_md, fields_json, entry_date, created_at, updated_at
             ) VALUES ('entry', 'Entry', 'files', '', ?1, '2026-01-01', 'now', 'now')",
            params![fields],
        )
        .expect("journal entry");
    }

    #[test]
    fn export_is_standard_readable_package_with_database_and_files() {
        let directory = TestDirectory::new("export");
        let db = state(&directory);
        let external = directory.0.join("source.txt");
        fs::write(&external, b"portable source").expect("external file");
        fs::create_dir_all(directory.0.join("media/doc")).expect("media directory");
        fs::write(directory.0.join("media/doc/image.png"), b"image").expect("media");
        add_source_data(&db, &external);
        let output = directory.0.join("workspace.zip");

        let result =
            export_workspace(&db, &PortabilityState::default(), &output).expect("workspace export");
        assert_eq!(result.attachment_count, 1);
        assert_eq!(result.media_count, 1);
        let extracted = validate_and_extract(&output).expect("valid portable archive");
        assert!(extracted.database.is_file());
        assert!(extracted.attachments.read_dir().unwrap().next().is_some());
        assert!(extracted.media.join("doc/image.png").is_file());
        assert!(
            fs::read_to_string(extracted._staging.path().join(ROOT).join(README_PATH))
                .unwrap()
                .contains("You do not need SoheiDesk")
        );
        let import_volume = TestDirectory::new("import-volume");
        let import_staging = validate_and_extract_in(&output, &import_volume.0)
            .expect("app-data-local import staging");
        assert_eq!(
            import_staging._staging.path().parent(),
            Some(import_volume.0.as_path())
        );
    }

    #[test]
    fn preview_and_import_require_explicit_replacement_and_rewrite_paths() {
        let source_directory = TestDirectory::new("source");
        let source_db = state(&source_directory);
        let external = source_directory.0.join("source.txt");
        fs::write(&external, b"portable source").expect("external file");
        add_source_data(&source_db, &external);
        let package = source_directory.0.join("workspace.zip");
        export_workspace(&source_db, &PortabilityState::default(), &package)
            .expect("workspace export");

        let target_directory = TestDirectory::new("target");
        let target_db = state(&target_directory);
        target_db
            .conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO settings(key, value) VALUES ('target', 'preserve')",
                [],
            )
            .expect("target data");
        let portability = PortabilityState::default();
        let preview = preview_import(&target_db, &portability, &package).expect("preview");
        assert!(preview.requires_replacement_confirmation);
        let search = SearchState::open(&target_directory.0).expect("search");
        let backups = BackupState::default();
        let error = import_workspace(
            &target_db,
            &backups,
            &portability,
            &search,
            &preview.token,
            false,
        )
        .expect_err("replacement confirmation");
        assert!(error.to_string().contains("explicit replacement"));

        let imported = import_workspace(
            &target_db,
            &backups,
            &portability,
            &search,
            &preview.token,
            true,
        )
        .expect("import workspace");
        assert_eq!(
            imported.emergency.kind,
            crate::backup::BackupKind::Emergency
        );
        let conn = target_db.conn.lock().expect("database lock");
        let marker: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key='portable_marker'",
                [],
                |row| row.get(0),
            )
            .expect("imported marker");
        assert_eq!(marker, "source");
        let document_path: String = conn
            .query_row(
                "SELECT last_path FROM documents WHERE id='doc'",
                [],
                |row| row.get(0),
            )
            .expect("document path");
        assert!(document_path.starts_with(
            target_directory
                .0
                .join("attachments")
                .to_string_lossy()
                .as_ref()
        ));
        assert!(Path::new(&document_path).is_file());
        let emergency_files = crate::backup::list_backups(&target_directory.0).unwrap();
        assert!(emergency_files
            .iter()
            .any(|backup| backup.kind == crate::backup::BackupKind::Emergency));
    }

    #[test]
    fn changed_archive_after_preview_is_rejected_without_touching_current_data() {
        let source_directory = TestDirectory::new("tamper-source");
        let source_db = state(&source_directory);
        let external = source_directory.0.join("source.txt");
        fs::write(&external, b"portable source").expect("external file");
        add_source_data(&source_db, &external);
        let package = source_directory.0.join("workspace.zip");
        export_workspace(&source_db, &PortabilityState::default(), &package)
            .expect("workspace export");

        let target_directory = TestDirectory::new("tamper-target");
        let target_db = state(&target_directory);
        let portability = PortabilityState::default();
        let preview = preview_import(&target_db, &portability, &package).expect("preview");
        let mut bytes = fs::read(&package).expect("archive bytes");
        bytes.push(0);
        fs::write(&package, bytes).expect("change archive");
        let search = SearchState::open(&target_directory.0).expect("search");
        let error = import_workspace(
            &target_db,
            &BackupState::default(),
            &portability,
            &search,
            &preview.token,
            false,
        )
        .expect_err("changed archive");
        assert!(error.to_string().contains("archive changed"));
        assert_eq!(
            workspace_counts(&target_db.conn.lock().unwrap())
                .unwrap()
                .total_records(),
            0
        );
    }

    #[test]
    fn export_refuses_to_replace_an_existing_archive() {
        let directory = TestDirectory::new("existing-export");
        let db = state(&directory);
        let output = directory.0.join("workspace.zip");
        fs::write(&output, b"existing archive").expect("existing archive");

        let error = export_workspace(&db, &PortabilityState::default(), &output)
            .expect_err("existing destination must be preserved");
        assert!(error.to_string().contains("already exists"));
        assert_eq!(fs::read(output).unwrap(), b"existing archive");
    }

    #[test]
    fn workspace_counts_support_every_historical_schema() {
        for version in 1..=db::migrations::latest_version() {
            let mut conn = Connection::open_in_memory().expect("database");
            db::migrations::apply_to_version(&mut conn, version).expect("historical schema");
            let counts = workspace_counts(&conn).expect("portable counts");
            assert_eq!(counts.total_records(), 0, "schema version {version}");
        }
    }

    #[test]
    fn manifest_reports_unknown_format_and_newer_schema_clearly() {
        let required = |path: &str| WorkspaceFile {
            path: path.into(),
            size: 0,
            sha256: "0".repeat(64),
        };
        let mut manifest = WorkspaceManifest {
            format_version: WORKSPACE_FORMAT_VERSION + 1,
            package_type: PACKAGE_TYPE.into(),
            id: Uuid::new_v4().to_string(),
            created_at: Utc::now().to_rfc3339(),
            app_version: "test".into(),
            schema_version: db::migrations::latest_version(),
            counts: WorkspaceCounts::default(),
            files: vec![required(DATABASE_PATH), required(README_PATH)],
            attachment_references: Vec::new(),
            missing_references: Vec::new(),
        };
        assert!(validate_manifest(&manifest)
            .expect_err("future format")
            .to_string()
            .contains("format"));

        manifest.format_version = WORKSPACE_FORMAT_VERSION;
        manifest.schema_version = db::migrations::latest_version() + 1;
        assert!(validate_manifest(&manifest)
            .expect_err("future schema")
            .to_string()
            .contains("update SoheiDesk"));
    }

    #[test]
    fn database_change_after_preview_requires_a_new_preview() {
        let source_directory = TestDirectory::new("changed-db-source");
        let source_db = state(&source_directory);
        let external = source_directory.0.join("source.txt");
        fs::write(&external, b"portable source").expect("external file");
        add_source_data(&source_db, &external);
        let package = source_directory.0.join("workspace.zip");
        export_workspace(&source_db, &PortabilityState::default(), &package)
            .expect("workspace export");

        let target_directory = TestDirectory::new("changed-db-target");
        let target_db = state(&target_directory);
        let portability = PortabilityState::default();
        let preview = preview_import(&target_db, &portability, &package).expect("preview");
        target_db
            .conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO settings(key, value) VALUES ('after_preview', 'preserve')",
                [],
            )
            .expect("new current data");
        let search = SearchState::open(&target_directory.0).expect("search");
        let error = import_workspace(
            &target_db,
            &BackupState::default(),
            &portability,
            &search,
            &preview.token,
            false,
        )
        .expect_err("stale preview");
        assert!(error.to_string().contains("changed after import preview"));
        let conn = target_db.conn.lock().unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT value FROM settings WHERE key='after_preview'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "preserve"
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM settings WHERE key='portable_marker'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );
    }
}
