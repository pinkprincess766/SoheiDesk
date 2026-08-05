//! Versioned, integrity-checked backups for the SQLite database and app media.
//!
//! SQLite snapshots and restores always use the Online Backup API. The live
//! database file is never copied directly because SoheiDesk runs in WAL mode.

use crate::db::{self, DbState};
use crate::error::{AppError, AppResult};
use chrono::{DateTime, Datelike, Local, Utc};
use rusqlite::backup::Backup;
use rusqlite::{Connection, DatabaseName, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{Emitter, Manager};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub const BACKUP_FORMAT_VERSION: u32 = 2;
const MIN_SUPPORTED_BACKUP_FORMAT_VERSION: u32 = 1;
const DATABASE_ARCHIVE_PATH: &str = "database/soheidesk.sqlite";
const SETTINGS_ARCHIVE_PATH: &str = "user-data/settings.json";
const TEMPLATES_ARCHIVE_PATH: &str = "user-data/templates.json";
const MANIFEST_ARCHIVE_PATH: &str = "manifest.json";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_BACKUP_BYTES: u64 = 50 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupKind {
    Daily,
    Manual,
    PreMigration,
    Emergency,
    Unknown,
}

impl BackupKind {
    fn file_slug(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Manual => "manual",
            Self::PreMigration => "pre-migration",
            Self::Emergency => "emergency",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub format_version: u32,
    pub id: String,
    pub kind: BackupKind,
    pub created_at: String,
    pub app_version: String,
    pub schema_version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_target: Option<i64>,
    pub files: Vec<ManifestFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupInfo {
    pub id: String,
    pub kind: BackupKind,
    pub created_at: String,
    pub size_bytes: u64,
    pub schema_version: i64,
    pub file_name: String,
    pub readable: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BackupRestoreResult {
    pub restored: BackupInfo,
    pub emergency: BackupInfo,
    pub reindexed_items: Option<u64>,
    pub warning: Option<String>,
}

#[derive(Default)]
pub struct BackupState {
    operation: Mutex<()>,
}

#[derive(Serialize)]
struct SettingDump {
    key: String,
    value: String,
}

#[derive(Serialize)]
struct UserTemplatesDump {
    journal_templates: Vec<serde_json::Value>,
    export_templates: Vec<serde_json::Value>,
}

struct PayloadFile {
    source: PathBuf,
    archive_path: String,
    size: u64,
    sha256: String,
}

struct StoredBackup {
    path: PathBuf,
    manifest: BackupManifest,
    size: u64,
}

struct ExtractedBackup {
    _staging: TempDir,
    database: PathBuf,
    media: PathBuf,
    attachments: PathBuf,
}

pub(crate) struct SnapshotRestoreResult {
    pub(crate) emergency: BackupInfo,
    pub(crate) reindexed_items: Option<u64>,
    pub(crate) warning: Option<String>,
}

struct TempDir {
    path: PathBuf,
}

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
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn zip_error(context: &str, error: zip::result::ZipError) -> AppError {
    AppError::Message(format!("{context}: {error}"))
}

fn create_private_file(path: &Path) -> AppResult<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

fn sync_directory(path: &Path) -> AppResult<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn backup_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("backups")
}

fn table_exists(conn: &Connection, table: &str) -> AppResult<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |row| row.get(0),
    )?)
}

pub(crate) fn create_sqlite_snapshot(source: &Connection, destination: &Path) -> AppResult<()> {
    let mut destination_conn = Connection::open(destination)?;
    // Copy in small steps so SQLite can release source locks between batches.
    let backup = Backup::new(source, &mut destination_conn)?;
    backup.run_to_completion(128, Duration::from_millis(25), None)?;
    drop(backup);
    verify_database(&destination_conn)?;
    Ok(())
}

pub(crate) fn verify_database(conn: &Connection) -> AppResult<()> {
    let result: String = conn.query_row("PRAGMA integrity_check(1)", [], |row| row.get(0))?;
    if result != "ok" {
        return Err(AppError::Message(format!(
            "SQLite integrity check failed: {result}"
        )));
    }
    Ok(())
}

fn dump_settings(conn: &Connection, destination: &Path) -> AppResult<()> {
    let mut values = Vec::new();
    if table_exists(conn, "settings")? {
        let mut stmt = conn.prepare("SELECT key, value FROM settings ORDER BY key")?;
        let rows = stmt.query_map([], |row| {
            Ok(SettingDump {
                key: row.get(0)?,
                value: row.get(1)?,
            })
        })?;
        for row in rows {
            values.push(row?);
        }
    }
    fs::write(destination, serde_json::to_vec_pretty(&values)?)?;
    Ok(())
}

fn dump_templates(conn: &Connection, destination: &Path) -> AppResult<()> {
    let mut journal_templates = Vec::new();
    if table_exists(conn, "templates")? {
        let mut stmt = conn.prepare(
            "SELECT id, name, description, category, fields_json, body_md,
                    default_tags_json, created_at, updated_at
             FROM templates WHERE is_builtin = 0 ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "description": row.get::<_, Option<String>>(2)?,
                "category": row.get::<_, Option<String>>(3)?,
                "fields_json": row.get::<_, String>(4)?,
                "body_md": row.get::<_, String>(5)?,
                "default_tags_json": row.get::<_, Option<String>>(6)?,
                "created_at": row.get::<_, String>(7)?,
                "updated_at": row.get::<_, String>(8)?,
            }))
        })?;
        for row in rows {
            journal_templates.push(row?);
        }
    }

    let mut export_templates = Vec::new();
    if table_exists(conn, "export_templates")? {
        let mut stmt = conn.prepare(
            "SELECT id, name, description, format, body, created_at, updated_at
             FROM export_templates WHERE is_builtin = 0 ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "description": row.get::<_, Option<String>>(2)?,
                "format": row.get::<_, String>(3)?,
                "body": row.get::<_, String>(4)?,
                "created_at": row.get::<_, String>(5)?,
                "updated_at": row.get::<_, String>(6)?,
            }))
        })?;
        for row in rows {
            export_templates.push(row?);
        }
    }

    let dump = UserTemplatesDump {
        journal_templates,
        export_templates,
    };
    fs::write(destination, serde_json::to_vec_pretty(&dump)?)?;
    Ok(())
}

pub(crate) fn hash_file(path: &Path) -> AppResult<(u64, String)> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(read as u64)
            .ok_or_else(|| AppError::Message("backup file is too large".into()))?;
        hasher.update(&buffer[..read]);
    }
    Ok((size, hex::encode(hasher.finalize())))
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
                "backup refuses symlink in app-managed data: {}",
                path.display()
            )));
        }
        if metadata.is_dir() {
            collect_tree_files(&path, root, archive_prefix, payload)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(AppError::Message(format!(
                "unsupported file in media: {}",
                path.display()
            )));
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| AppError::Message("invalid backup tree path".into()))?;
        let relative = relative
            .to_str()
            .ok_or_else(|| AppError::Message("media filename is not valid UTF-8".into()))?
            .replace('\\', "/");
        let (size, sha256) = hash_file(&path)?;
        payload.push(PayloadFile {
            source: path,
            archive_path: format!("{archive_prefix}/{relative}"),
            size,
            sha256,
        });
    }
    Ok(())
}

fn payload_file(source: PathBuf, archive_path: &str) -> AppResult<PayloadFile> {
    let (size, sha256) = hash_file(&source)?;
    Ok(PayloadFile {
        source,
        archive_path: archive_path.into(),
        size,
        sha256,
    })
}

fn write_archive(path: &Path, payload: &[PayloadFile], manifest: &BackupManifest) -> AppResult<()> {
    let file = create_private_file(path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o600);
    let mut buffer = [0_u8; 64 * 1024];

    for item in payload {
        zip.start_file(&item.archive_path, options)
            .map_err(|error| zip_error("start backup file", error))?;
        let mut source = File::open(&item.source)?;
        loop {
            let read = source.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            zip.write_all(&buffer[..read])?;
        }
    }
    zip.start_file(MANIFEST_ARCHIVE_PATH, options)
        .map_err(|error| zip_error("start backup manifest", error))?;
    zip.write_all(&serde_json::to_vec_pretty(manifest)?)?;
    let output = zip
        .finish()
        .map_err(|error| zip_error("finish backup archive", error))?;
    output.sync_all()?;
    Ok(())
}

pub fn create_archive(
    data_dir: &Path,
    conn: &Connection,
    kind: BackupKind,
    migration_target: Option<i64>,
) -> AppResult<BackupInfo> {
    if kind == BackupKind::Unknown {
        return Err(AppError::Message("invalid backup kind".into()));
    }
    let backups = backup_dir(data_dir);
    fs::create_dir_all(&backups)?;
    let staging = TempDir::create(&backups, "backup-staging")?;
    let snapshot = staging.path().join("soheidesk.sqlite");
    let settings = staging.path().join("settings.json");
    let templates = staging.path().join("templates.json");

    create_sqlite_snapshot(conn, &snapshot)?;
    let snapshot_conn = Connection::open_with_flags(&snapshot, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let schema_version = db::migrations::current_version(&snapshot_conn)?;
    dump_settings(&snapshot_conn, &settings)?;
    dump_templates(&snapshot_conn, &templates)?;
    drop(snapshot_conn);

    let mut payload = vec![
        payload_file(snapshot, DATABASE_ARCHIVE_PATH)?,
        payload_file(settings, SETTINGS_ARCHIVE_PATH)?,
        payload_file(templates, TEMPLATES_ARCHIVE_PATH)?,
    ];
    collect_tree_files(
        &data_dir.join("media"),
        &data_dir.join("media"),
        "media",
        &mut payload,
    )?;
    collect_tree_files(
        &data_dir.join("attachments"),
        &data_dir.join("attachments"),
        "attachments",
        &mut payload,
    )?;
    payload.sort_by(|left, right| left.archive_path.cmp(&right.archive_path));

    let total_size = payload.iter().try_fold(0_u64, |total, item| {
        total
            .checked_add(item.size)
            .ok_or_else(|| AppError::Message("backup is too large".into()))
    })?;
    if total_size > MAX_BACKUP_BYTES {
        return Err(AppError::Message(format!(
            "backup payload exceeds {} GiB safety limit",
            MAX_BACKUP_BYTES / 1024 / 1024 / 1024
        )));
    }

    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let manifest = BackupManifest {
        format_version: BACKUP_FORMAT_VERSION,
        id: id.clone(),
        kind,
        created_at: now.to_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        schema_version,
        migration_target,
        files: payload
            .iter()
            .map(|item| ManifestFile {
                path: item.archive_path.clone(),
                size: item.size,
                sha256: item.sha256.clone(),
            })
            .collect(),
    };
    let file_name = format!(
        "soheidesk-{}-{}-{}.zip",
        kind.file_slug(),
        now.format("%Y%m%dT%H%M%SZ"),
        &id[..8]
    );
    let final_path = backups.join(&file_name);
    let partial_path = backups.join(format!(".{file_name}.partial"));
    let result = write_archive(&partial_path, &payload, &manifest);
    if let Err(error) = result {
        let _ = fs::remove_file(&partial_path);
        return Err(error);
    }
    fs::rename(&partial_path, &final_path)?;
    sync_directory(&backups)?;

    let info = stored_to_info(&StoredBackup {
        size: fs::metadata(&final_path)?.len(),
        path: final_path.clone(),
        manifest,
    });
    if let Err(error) = validate_and_extract_path(&final_path) {
        let _ = fs::remove_file(&final_path);
        return Err(AppError::Message(format!(
            "new backup failed verification and was removed: {error}"
        )));
    }
    Ok(info)
}

fn validate_archive_path(path: &str) -> AppResult<()> {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') {
        return Err(AppError::Message(format!("unsafe backup path: {path}")));
    }
    let parsed = Path::new(path);
    for component in parsed.components() {
        let Component::Normal(name) = component else {
            return Err(AppError::Message(format!("unsafe backup path: {path}")));
        };
        let name = name
            .to_str()
            .ok_or_else(|| AppError::Message(format!("unsafe backup path: {path}")))?;
        let upper_stem = name
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        let windows_reserved = matches!(
            upper_stem.as_str(),
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
        );
        if name.contains(':')
            || name.chars().any(char::is_control)
            || name.ends_with(' ')
            || name.ends_with('.')
            || windows_reserved
        {
            return Err(AppError::Message(format!("unsafe backup path: {path}")));
        }
    }
    Ok(())
}

fn validate_manifest(manifest: &BackupManifest) -> AppResult<()> {
    if !(MIN_SUPPORTED_BACKUP_FORMAT_VERSION..=BACKUP_FORMAT_VERSION)
        .contains(&manifest.format_version)
    {
        return Err(AppError::Message(format!(
            "unsupported backup format {} (supported: {}..={})",
            manifest.format_version, MIN_SUPPORTED_BACKUP_FORMAT_VERSION, BACKUP_FORMAT_VERSION
        )));
    }
    if manifest.kind == BackupKind::Unknown || Uuid::parse_str(&manifest.id).is_err() {
        return Err(AppError::Message("invalid backup manifest identity".into()));
    }
    DateTime::parse_from_rfc3339(&manifest.created_at)
        .map_err(|error| AppError::Message(format!("invalid backup timestamp: {error}")))?;
    if manifest.schema_version < 0 {
        return Err(AppError::Message("invalid negative schema version".into()));
    }
    if manifest.schema_version > db::migrations::latest_version() {
        return Err(AppError::Message(format!(
            "backup schema {} is newer than this app supports ({})",
            manifest.schema_version,
            db::migrations::latest_version()
        )));
    }

    let mut paths = HashSet::new();
    let mut portable_paths = HashSet::new();
    let mut total = 0_u64;
    for file in &manifest.files {
        validate_archive_path(&file.path)?;
        let allowed = file.path == DATABASE_ARCHIVE_PATH
            || file.path == SETTINGS_ARCHIVE_PATH
            || file.path == TEMPLATES_ARCHIVE_PATH
            || file.path.starts_with("media/")
            || (manifest.format_version >= 2 && file.path.starts_with("attachments/"));
        if !allowed {
            return Err(AppError::Message(format!(
                "unexpected backup payload: {}",
                file.path
            )));
        }
        if !paths.insert(file.path.as_str()) {
            return Err(AppError::Message(format!(
                "duplicate backup payload: {}",
                file.path
            )));
        }
        if !portable_paths.insert(file.path.to_ascii_lowercase()) {
            return Err(AppError::Message(format!(
                "case-insensitive backup path collision: {}",
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
            .ok_or_else(|| AppError::Message("backup is too large".into()))?;
    }
    for required in [
        DATABASE_ARCHIVE_PATH,
        SETTINGS_ARCHIVE_PATH,
        TEMPLATES_ARCHIVE_PATH,
    ] {
        if !paths.contains(required) {
            return Err(AppError::Message(format!(
                "backup is missing required file: {required}"
            )));
        }
    }
    if total > MAX_BACKUP_BYTES {
        return Err(AppError::Message("backup exceeds safety limit".into()));
    }
    Ok(())
}

fn read_manifest(path: &Path) -> AppResult<BackupManifest> {
    let file = File::open(path)?;
    let mut zip = ZipArchive::new(file).map_err(|error| zip_error("open backup", error))?;
    if zip.len() > 100_000 {
        return Err(AppError::Message("backup contains too many entries".into()));
    }
    let manifest_count = (0..zip.len())
        .filter(|index| {
            zip.by_index(*index)
                .map(|entry| entry.name() == MANIFEST_ARCHIVE_PATH)
                .unwrap_or(false)
        })
        .count();
    if manifest_count != 1 {
        return Err(AppError::Message(
            "backup must contain exactly one manifest".into(),
        ));
    }
    let mut entry = zip
        .by_name(MANIFEST_ARCHIVE_PATH)
        .map_err(|error| zip_error("read backup manifest", error))?;
    if entry.size() > MAX_MANIFEST_BYTES {
        return Err(AppError::Message("backup manifest is too large".into()));
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut bytes)?;
    let manifest: BackupManifest = serde_json::from_slice(&bytes)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn scan_backups(data_dir: &Path) -> AppResult<Vec<StoredBackup>> {
    let directory = backup_dir(data_dir);
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut backups = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type()?.is_file()
            || path.extension().and_then(|value| value.to_str()) != Some("zip")
        {
            continue;
        }
        if let Ok(manifest) = read_manifest(&path) {
            backups.push(StoredBackup {
                size: entry.metadata()?.len(),
                path,
                manifest,
            });
        }
    }
    backups.sort_by(|left, right| right.manifest.created_at.cmp(&left.manifest.created_at));
    Ok(backups)
}

fn unreadable_backup(path: &Path, error: AppError) -> BackupInfo {
    BackupInfo {
        id: path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("unreadable-backup")
            .into(),
        kind: BackupKind::Unknown,
        created_at: String::new(),
        size_bytes: fs::metadata(path).map(|value| value.len()).unwrap_or(0),
        schema_version: -1,
        file_name: path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("unreadable.zip")
            .into(),
        readable: false,
        error: Some(error.to_string()),
    }
}

fn stored_to_info(stored: &StoredBackup) -> BackupInfo {
    BackupInfo {
        id: stored.manifest.id.clone(),
        kind: stored.manifest.kind,
        created_at: stored.manifest.created_at.clone(),
        size_bytes: stored.size,
        schema_version: stored.manifest.schema_version,
        file_name: stored
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("backup.zip")
            .into(),
        readable: true,
        error: None,
    }
}

pub fn list_backups(data_dir: &Path) -> AppResult<Vec<BackupInfo>> {
    let directory = backup_dir(data_dir);
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type()?.is_file()
            || path.extension().and_then(|value| value.to_str()) != Some("zip")
        {
            continue;
        }
        match read_manifest(&path) {
            Ok(manifest) => result.push(stored_to_info(&StoredBackup {
                size: entry.metadata()?.len(),
                path,
                manifest,
            })),
            Err(error) => result.push(unreadable_backup(&path, error)),
        }
    }
    result.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(result)
}

fn find_backup(data_dir: &Path, id: &str) -> AppResult<StoredBackup> {
    let matching: Vec<_> = scan_backups(data_dir)?
        .into_iter()
        .filter(|backup| backup.manifest.id == id)
        .collect();
    match matching.len() {
        1 => matching
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Message("backup not found".into())),
        0 => Err(AppError::Message("backup not found".into())),
        _ => Err(AppError::Message("duplicate backup id".into())),
    }
}

fn validate_and_extract_path(path: &Path) -> AppResult<ExtractedBackup> {
    // This is the restore trust boundary: reject unknown entries and unsafe
    // paths, then stream every payload through size and SHA-256 verification.
    let manifest = read_manifest(path)?;
    let staging = TempDir::create(
        path.parent()
            .ok_or_else(|| AppError::Message("backup has no parent directory".into()))?,
        "restore-staging",
    )?;
    let file = File::open(path)?;
    let mut zip = ZipArchive::new(file).map_err(|error| zip_error("open backup", error))?;
    if zip.len() != manifest.files.len() + 1 {
        return Err(AppError::Message(
            "archive contents do not match manifest".into(),
        ));
    }
    let expected: HashMap<&str, &ManifestFile> = manifest
        .files
        .iter()
        .map(|entry| (entry.path.as_str(), entry))
        .collect();
    let mut archive_names = HashSet::new();
    for index in 0..zip.len() {
        let entry = zip
            .by_index(index)
            .map_err(|error| zip_error("read backup entry", error))?;
        let name = entry.name().to_string();
        validate_archive_path(&name)?;
        if !archive_names.insert(name.clone()) {
            return Err(AppError::Message(format!(
                "duplicate archive entry: {name}"
            )));
        }
        if name != MANIFEST_ARCHIVE_PATH && !expected.contains_key(name.as_str()) {
            return Err(AppError::Message(format!("unknown archive entry: {name}")));
        }
    }
    if archive_names.len() != expected.len() + 1 {
        return Err(AppError::Message(
            "archive contents do not match manifest".into(),
        ));
    }

    for declared in &manifest.files {
        let mut source = zip
            .by_name(&declared.path)
            .map_err(|error| zip_error("read backup payload", error))?;
        if source.size() != declared.size || source.size() > MAX_BACKUP_BYTES {
            return Err(AppError::Message(format!(
                "size mismatch for {}",
                declared.path
            )));
        }
        let destination = staging.path().join(&declared.path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = File::create(&destination)?;
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
                .ok_or_else(|| AppError::Message("backup payload is too large".into()))?;
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

    let database = staging.path().join(DATABASE_ARCHIVE_PATH);
    let database_conn = Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    verify_database(&database_conn)?;
    let schema_version = db::migrations::current_version(&database_conn)?;
    if schema_version != manifest.schema_version {
        return Err(AppError::Message(format!(
            "database schema version {schema_version} does not match manifest {}",
            manifest.schema_version
        )));
    }
    drop(database_conn);

    for json_path in [SETTINGS_ARCHIVE_PATH, TEMPLATES_ARCHIVE_PATH] {
        let bytes = fs::read(staging.path().join(json_path))?;
        let _: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|error| AppError::Message(format!("invalid {json_path}: {error}")))?;
    }

    let media = staging.path().join("media");
    fs::create_dir_all(&media)?;
    let attachments = staging.path().join("attachments");
    fs::create_dir_all(&attachments)?;
    Ok(ExtractedBackup {
        _staging: staging,
        database,
        media,
        attachments,
    })
}

fn local_day(timestamp: &str) -> AppResult<String> {
    let parsed = DateTime::parse_from_rfc3339(timestamp)
        .map_err(|error| AppError::Message(format!("invalid backup timestamp: {error}")))?;
    Ok(parsed.with_timezone(&Local).format("%Y-%m-%d").to_string())
}

fn retention_keep_ids(backups: &[StoredBackup]) -> AppResult<HashSet<String>> {
    let daily: Vec<_> = backups
        .iter()
        .filter(|backup| backup.manifest.kind == BackupKind::Daily)
        .collect();
    let mut kept = HashSet::new();
    let mut recent_days = HashSet::new();
    for backup in &daily {
        let day = local_day(&backup.manifest.created_at)?;
        if recent_days.contains(&day) || recent_days.len() >= 7 {
            continue;
        }
        recent_days.insert(day);
        kept.insert(backup.manifest.id.clone());
    }

    // Weekly representatives are selected only after the seven recent daily
    // slots, so the policy keeps up to eleven distinct automatic archives.
    let mut weekly = HashSet::new();
    for backup in &daily {
        if kept.contains(&backup.manifest.id) {
            continue;
        }
        let parsed = DateTime::parse_from_rfc3339(&backup.manifest.created_at)
            .map_err(|error| AppError::Message(format!("invalid backup timestamp: {error}")))?
            .with_timezone(&Local);
        let iso = parsed.iso_week();
        let key = (iso.year(), iso.week());
        if weekly.contains(&key) || weekly.len() >= 4 {
            continue;
        }
        weekly.insert(key);
        kept.insert(backup.manifest.id.clone());
    }
    Ok(kept)
}

pub fn apply_retention(data_dir: &Path) -> AppResult<()> {
    let backups = scan_backups(data_dir)?;
    let keep = retention_keep_ids(&backups)?;
    let mut removed = false;
    for backup in backups {
        if backup.manifest.kind == BackupKind::Daily && !keep.contains(&backup.manifest.id) {
            fs::remove_file(backup.path)?;
            removed = true;
        }
    }
    if removed {
        sync_directory(&backup_dir(data_dir))?;
    }
    Ok(())
}

fn has_daily_backup_today(data_dir: &Path) -> AppResult<bool> {
    let today = Local::now().format("%Y-%m-%d").to_string();
    for backup in scan_backups(data_dir)? {
        if backup.manifest.kind == BackupKind::Daily
            && local_day(&backup.manifest.created_at)? == today
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn create_manual(db: &DbState, state: &BackupState) -> AppResult<BackupInfo> {
    let _operation = state
        .operation
        .lock()
        .map_err(|_| AppError::Message("backup lock poisoned".into()))?;
    let _media = db
        .media
        .lock()
        .map_err(|_| AppError::Message("media lock poisoned".into()))?;
    let conn = db
        .conn
        .lock()
        .map_err(|_| AppError::Message("database lock poisoned".into()))?;
    create_archive(&db.data_dir, &conn, BackupKind::Manual, None)
}

pub fn create_daily_if_due(db: &DbState, state: &BackupState) -> AppResult<Option<BackupInfo>> {
    let _operation = state
        .operation
        .lock()
        .map_err(|_| AppError::Message("backup lock poisoned".into()))?;
    if has_daily_backup_today(&db.data_dir)? {
        return Ok(None);
    }
    let _media = db
        .media
        .lock()
        .map_err(|_| AppError::Message("media lock poisoned".into()))?;
    let conn = db
        .conn
        .lock()
        .map_err(|_| AppError::Message("database lock poisoned".into()))?;
    let info = create_archive(&db.data_dir, &conn, BackupKind::Daily, None)?;
    drop(conn);
    apply_retention(&db.data_dir)?;
    Ok(Some(info))
}

fn replace_data_tree(data_dir: &Path, name: &str, selected: &Path) -> AppResult<Option<PathBuf>> {
    let current = data_dir.join(name);
    let rollback = data_dir.join(format!(
        ".{name}-before-restore-{}",
        Uuid::new_v4().simple()
    ));
    let had_current = current.exists();
    if had_current {
        fs::rename(&current, &rollback)?;
    }
    if let Err(error) = fs::rename(selected, &current) {
        if had_current {
            if let Err(rollback_error) = fs::rename(&rollback, &current) {
                return Err(AppError::Message(format!(
                    "{name} replacement failed ({error}); restoring current data also failed ({rollback_error})"
                )));
            }
        }
        return Err(error.into());
    }
    Ok(had_current.then_some(rollback))
}

fn rollback_data_tree(data_dir: &Path, name: &str, rollback: Option<&Path>) -> AppResult<()> {
    let current = data_dir.join(name);
    let failed_restore = data_dir.join(format!(
        ".{name}-from-failed-restore-{}",
        Uuid::new_v4().simple()
    ));
    let had_current = current.exists();
    if had_current {
        // Keep the failed restore intact until the previous data tree is back in place.
        fs::rename(&current, &failed_restore)?;
    }
    if let Some(previous) = rollback {
        if let Err(error) = fs::rename(previous, &current) {
            let recovery = if had_current {
                fs::rename(&failed_restore, &current).err()
            } else {
                None
            };
            return Err(match recovery {
                Some(recovery_error) => AppError::Message(format!(
                    "previous {name} restore failed ({error}); keeping restored data also failed ({recovery_error})"
                )),
                None => AppError::Message(format!(
                    "previous {name} restore failed; restored data was preserved: {error}"
                )),
            });
        }
    }
    if had_current {
        fs::remove_dir_all(failed_restore)?;
    }
    Ok(())
}

pub fn restore_backup(
    db: &DbState,
    state: &BackupState,
    search: &crate::search::SearchState,
    backup_id: &str,
) -> AppResult<BackupRestoreResult> {
    let _operation = state
        .operation
        .lock()
        .map_err(|_| AppError::Message("backup lock poisoned".into()))?;
    let selected_stored = find_backup(&db.data_dir, backup_id)?;
    // Validate the selected archive before locking or changing current data.
    let selected = validate_and_extract_path(&selected_stored.path)?;
    let outcome = restore_snapshot_paths(
        db,
        search,
        &selected.database,
        &selected.media,
        &selected.attachments,
        None,
    )?;
    Ok(BackupRestoreResult {
        restored: stored_to_info(&selected_stored),
        emergency: outcome.emergency,
        reindexed_items: outcome.reindexed_items,
        warning: outcome.warning,
    })
}

pub(crate) fn restore_external_snapshot(
    db: &DbState,
    state: &BackupState,
    search: &crate::search::SearchState,
    database: &Path,
    media: &Path,
    attachments: &Path,
    expected_current_sha256: &str,
) -> AppResult<SnapshotRestoreResult> {
    let _operation = state
        .operation
        .lock()
        .map_err(|_| AppError::Message("backup lock poisoned".into()))?;
    restore_snapshot_paths(
        db,
        search,
        database,
        media,
        attachments,
        Some(expected_current_sha256),
    )
}

fn restore_snapshot_paths(
    db: &DbState,
    search: &crate::search::SearchState,
    selected_database: &Path,
    selected_media: &Path,
    selected_attachments: &Path,
    expected_current_sha256: Option<&str>,
) -> AppResult<SnapshotRestoreResult> {
    let _media = db
        .media
        .lock()
        .map_err(|_| AppError::Message("media lock poisoned".into()))?;
    let mut conn = db
        .conn
        .lock()
        .map_err(|_| AppError::Message("database lock poisoned".into()))?;
    let emergency_info = create_archive(&db.data_dir, &conn, BackupKind::Emergency, None)?;
    let emergency_stored = find_backup(&db.data_dir, &emergency_info.id)?;
    let emergency = validate_and_extract_path(&emergency_stored.path)?;
    if let Some(expected) = expected_current_sha256 {
        let (_, actual) = hash_file(&emergency.database)?;
        if actual != expected {
            return Err(AppError::Message(
                "workspace changed after import preview; run preview again before importing".into(),
            ));
        }
    }

    let media_rollback = replace_data_tree(&db.data_dir, "media", selected_media)?;
    let attachments_rollback = match replace_data_tree(
        &db.data_dir,
        "attachments",
        selected_attachments,
    ) {
        Ok(rollback) => rollback,
        Err(error) => {
            let media_status = rollback_data_tree(&db.data_dir, "media", media_rollback.as_deref());
            return match media_status {
                Ok(()) => Err(error),
                Err(rollback_error) => Err(AppError::Message(format!(
                    "attachments replacement failed ({error}); media rollback also failed ({rollback_error})"
                ))),
            };
        }
    };
    let restore_result = conn
        .restore(
            DatabaseName::Main,
            selected_database,
            None::<fn(rusqlite::backup::Progress)>,
        )
        .map_err(AppError::from)
        .and_then(|_| {
            db::migrations::apply_with_hook(&mut conn, |connection, _from, target| {
                create_archive(
                    &db.data_dir,
                    connection,
                    BackupKind::PreMigration,
                    Some(target),
                )?;
                Ok(())
            })
        })
        .and_then(|_| {
            conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
            verify_database(&conn)
        });
    if let Err(error) = restore_result {
        let database_rollback = conn
            .restore(
                DatabaseName::Main,
                &emergency.database,
                None::<fn(rusqlite::backup::Progress)>,
            )
            .map_err(AppError::from)
            .and_then(|_| {
                conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
                verify_database(&conn)
            });
        let media_result = rollback_data_tree(&db.data_dir, "media", media_rollback.as_deref());
        let attachments_result =
            rollback_data_tree(&db.data_dir, "attachments", attachments_rollback.as_deref());
        return match (database_rollback, media_result, attachments_result) {
            (Ok(()), Ok(()), Ok(())) => Err(AppError::Message(format!(
                "restore failed and current data was rolled back: {error}"
            ))),
            (database_result, media_result, attachments_result) => {
                let database_status = database_result
                    .err()
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "ok".into());
                let media_status = media_result
                    .err()
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "ok".into());
                let attachments_status = attachments_result
                    .err()
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "ok".into());
                Err(AppError::Message(format!(
                    "restore failed ({error}); rollback was incomplete (database: {database_status}; media: {media_status}; attachments: {attachments_status}). Emergency backup: {}",
                    emergency_info.file_name
                )))
            }
        };
    }
    drop(conn);
    let mut cleanup_warnings = Vec::new();
    for (name, rollback) in [
        ("media", media_rollback),
        ("attachments", attachments_rollback),
    ] {
        if let Some(previous) = rollback {
            if let Err(error) = fs::remove_dir_all(previous) {
                cleanup_warnings.push(format!("Old {name} cleanup failed: {error}"));
            }
        }
    }

    let mut warnings = Vec::new();
    if let Err(error) = crate::templates::seed_builtins(db) {
        warnings.push(format!(
            "Built-in journal templates were not refreshed: {error}"
        ));
    }
    if let Err(error) = crate::export::seed_export_templates(db) {
        warnings.push(format!(
            "Built-in export templates were not refreshed: {error}"
        ));
    }
    let reindexed_items = match search.reindex_all(db) {
        Ok(count) => Some(count),
        Err(error) => {
            warnings.push(format!(
                "Data was restored, but the search index could not be rebuilt: {error}"
            ));
            None
        }
    };
    warnings.extend(cleanup_warnings);
    let warning = if warnings.is_empty() {
        None
    } else {
        Some(warnings.join("; "))
    };
    Ok(SnapshotRestoreResult {
        emergency: emergency_info,
        reindexed_items,
        warning,
    })
}

pub fn start_scheduler(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(10));
        loop {
            if let (Some(db), Some(backups)) =
                (app.try_state::<DbState>(), app.try_state::<BackupState>())
            {
                if let Err(error) = create_daily_if_due(&db, &backups) {
                    eprintln!("daily backup failed: {error}");
                    let _ = app.emit("backup-error", error.to_string());
                }
            }
            std::thread::sleep(Duration::from_secs(60 * 60));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir()
                .join(format!("soheidesk-backup-test-{}", Uuid::new_v4().simple()));
            fs::create_dir(&path).expect("test directory");
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn test_database(path: &Path) -> Connection {
        let conn = db::open(path).expect("database");
        conn.execute(
            "INSERT INTO settings(key, value) VALUES ('ui_theme', 'dark')",
            [],
        )
        .expect("setting");
        conn.execute(
            "INSERT INTO templates(
                id, name, is_builtin, fields_json, body_md, created_at, updated_at
             ) VALUES ('mine', 'Mine', 0, '[]', '# Body', 't', 't')",
            [],
        )
        .expect("template");
        conn
    }

    fn tamper_settings_payload(path: &Path) {
        let input = File::open(path).expect("backup file");
        let mut archive = ZipArchive::new(input).expect("backup zip");
        let mut entries = Vec::new();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).expect("zip entry");
            let name = entry.name().to_string();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).expect("entry bytes");
            if name == SETTINGS_ARCHIVE_PATH {
                bytes[0] ^= 1;
            }
            entries.push((name, bytes));
        }
        drop(archive);

        let replacement = path.with_extension("replacement");
        let mut writer = ZipWriter::new(File::create(&replacement).expect("replacement"));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, bytes) in entries {
            writer.start_file(name, options).expect("start entry");
            writer.write_all(&bytes).expect("write entry");
        }
        writer.finish().expect("finish replacement");
        fs::rename(replacement, path).expect("replace backup");
    }

    #[test]
    fn backup_contains_database_media_and_user_data_but_not_search_index() {
        let directory = TestDir::new();
        fs::create_dir_all(directory.0.join("media/doc-1")).expect("media directory");
        fs::write(directory.0.join("media/doc-1/page.png"), b"image").expect("media");
        fs::create_dir_all(directory.0.join("attachments")).expect("attachments directory");
        fs::write(directory.0.join("attachments/source.csv"), b"data").expect("attachment");
        fs::create_dir_all(directory.0.join("tantivy_index")).expect("index directory");
        fs::write(directory.0.join("tantivy_index/index"), b"rebuild me").expect("index");
        let database_path = directory.0.join("soheidesk.sqlite");
        let conn = test_database(&database_path);
        assert!(database_path.with_extension("sqlite-wal").exists());

        let info =
            create_archive(&directory.0, &conn, BackupKind::Manual, None).expect("create backup");
        let stored = find_backup(&directory.0, &info.id).expect("stored backup");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&stored.path)
                    .expect("backup metadata")
                    .permissions()
                    .mode()
                    & 0o077,
                0
            );
        }
        let extracted = validate_and_extract_path(&stored.path).expect("valid backup");
        assert!(extracted.database.exists());
        assert!(extracted.media.join("doc-1/page.png").exists());
        assert!(extracted.attachments.join("source.csv").exists());
        let snapshot = Connection::open(&extracted.database).expect("snapshot database");
        let theme: String = snapshot
            .query_row(
                "SELECT value FROM settings WHERE key='ui_theme'",
                [],
                |row| row.get(0),
            )
            .expect("WAL-backed setting in snapshot");
        assert_eq!(theme, "dark");
        assert!(stored
            .manifest
            .files
            .iter()
            .any(|file| file.path == SETTINGS_ARCHIVE_PATH));
        assert!(stored
            .manifest
            .files
            .iter()
            .any(|file| file.path == TEMPLATES_ARCHIVE_PATH));
        assert!(!stored
            .manifest
            .files
            .iter()
            .any(|file| file.path.contains("tantivy")));
    }

    #[test]
    fn damaged_archive_is_rejected() {
        let directory = TestDir::new();
        let conn = test_database(&directory.0.join("soheidesk.sqlite"));
        let info =
            create_archive(&directory.0, &conn, BackupKind::Manual, None).expect("create backup");
        let stored = find_backup(&directory.0, &info.id).expect("stored backup");
        let mut bytes = fs::read(&stored.path).expect("backup bytes");
        bytes.truncate(bytes.len() / 2);
        fs::write(&stored.path, bytes).expect("damage backup");
        assert!(validate_and_extract_path(&stored.path).is_err());
        let listed = list_backups(&directory.0).expect("list backups");
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].readable);
    }

    #[test]
    fn changed_payload_is_rejected_by_checksum() {
        let directory = TestDir::new();
        let conn = test_database(&directory.0.join("soheidesk.sqlite"));
        let info =
            create_archive(&directory.0, &conn, BackupKind::Manual, None).expect("create backup");
        let stored = find_backup(&directory.0, &info.id).expect("stored backup");
        tamper_settings_payload(&stored.path);

        let error = match validate_and_extract_path(&stored.path) {
            Ok(_) => panic!("changed payload passed validation"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("checksum mismatch"));
    }

    #[test]
    fn media_rollback_restores_previous_data_without_discarding_failed_restore() {
        let directory = TestDir::new();
        let current = directory.0.join("media");
        let previous = directory.0.join("previous-media");
        fs::create_dir(&current).expect("current media");
        fs::write(current.join("state.txt"), b"selected").expect("selected media");
        fs::create_dir(&previous).expect("previous media");
        fs::write(previous.join("state.txt"), b"original").expect("original media");

        rollback_data_tree(&directory.0, "media", Some(&previous)).expect("media rollback");
        assert_eq!(
            fs::read(current.join("state.txt")).expect("restored media"),
            b"original"
        );

        fs::remove_dir_all(&current).expect("remove restored media");
        fs::create_dir(&current).expect("recreate current media");
        fs::write(current.join("state.txt"), b"selected").expect("selected media");
        let missing = directory.0.join("missing-previous-media");
        rollback_data_tree(&directory.0, "media", Some(&missing))
            .expect_err("missing previous media");
        assert_eq!(
            fs::read(current.join("state.txt")).expect("preserved selected media"),
            b"selected"
        );
    }

    #[test]
    fn restore_checks_integrity_before_touching_current_data() {
        let directory = TestDir::new();
        let conn = test_database(&directory.0.join("soheidesk.sqlite"));
        let db = DbState {
            conn: Mutex::new(conn),
            media: Mutex::new(()),
            data_dir: directory.0.clone(),
        };
        let state = BackupState::default();
        let search = crate::search::SearchState::open(&directory.0).expect("search index");
        let backup = create_manual(&db, &state).expect("backup");
        let stored = find_backup(&directory.0, &backup.id).expect("stored backup");
        tamper_settings_payload(&stored.path);
        db.conn
            .lock()
            .expect("database lock")
            .execute("UPDATE settings SET value='light' WHERE key='ui_theme'", [])
            .expect("change current data");

        let error = restore_backup(&db, &state, &search, &backup.id)
            .expect_err("corrupted backup must not restore");
        assert!(error.to_string().contains("checksum mismatch"));
        let theme: String = db
            .conn
            .lock()
            .expect("database lock")
            .query_row(
                "SELECT value FROM settings WHERE key='ui_theme'",
                [],
                |row| row.get(0),
            )
            .expect("current theme");
        assert_eq!(theme, "light");
        assert!(!scan_backups(&directory.0)
            .expect("backups")
            .iter()
            .any(|stored| stored.manifest.kind == BackupKind::Emergency));
    }

    #[test]
    fn daily_backup_is_created_only_once_per_local_day() {
        let directory = TestDir::new();
        let conn = test_database(&directory.0.join("soheidesk.sqlite"));
        let db = DbState {
            conn: Mutex::new(conn),
            media: Mutex::new(()),
            data_dir: directory.0.clone(),
        };
        let state = BackupState::default();

        assert!(create_daily_if_due(&db, &state)
            .expect("first daily")
            .is_some());
        assert!(create_daily_if_due(&db, &state)
            .expect("second daily")
            .is_none());
        assert_eq!(
            scan_backups(&directory.0)
                .expect("backups")
                .into_iter()
                .filter(|backup| backup.manifest.kind == BackupKind::Daily)
                .count(),
            1
        );
    }

    #[test]
    fn restore_round_trip_creates_emergency_copy_and_restores_media() {
        let directory = TestDir::new();
        fs::create_dir_all(directory.0.join("media")).expect("media directory");
        fs::write(directory.0.join("media/state.txt"), b"original").expect("original media");
        fs::create_dir_all(directory.0.join("attachments")).expect("attachments directory");
        fs::write(
            directory.0.join("attachments/state.txt"),
            b"original attachment",
        )
        .expect("original attachment");
        let conn = test_database(&directory.0.join("soheidesk.sqlite"));
        let db = DbState {
            conn: Mutex::new(conn),
            media: Mutex::new(()),
            data_dir: directory.0.clone(),
        };
        let state = BackupState::default();
        let search = crate::search::SearchState::open(&directory.0).expect("search index");
        let original = create_manual(&db, &state).expect("original backup");

        {
            let conn = db.conn.lock().expect("database lock");
            conn.execute("UPDATE settings SET value='light' WHERE key='ui_theme'", [])
                .expect("change setting");
        }
        fs::write(directory.0.join("media/state.txt"), b"changed").expect("changed media");
        fs::write(
            directory.0.join("attachments/state.txt"),
            b"changed attachment",
        )
        .expect("changed attachment");

        let restored = restore_backup(&db, &state, &search, &original.id).expect("restore");
        assert_eq!(restored.emergency.kind, BackupKind::Emergency);
        assert_eq!(restored.reindexed_items, Some(0));
        let theme: String = db
            .conn
            .lock()
            .expect("database lock")
            .query_row(
                "SELECT value FROM settings WHERE key='ui_theme'",
                [],
                |row| row.get(0),
            )
            .expect("theme");
        assert_eq!(theme, "dark");
        assert_eq!(
            fs::read(directory.0.join("media/state.txt")).expect("restored media"),
            b"original"
        );
        assert_eq!(
            fs::read(directory.0.join("attachments/state.txt")).expect("restored attachment"),
            b"original attachment"
        );
        let emergency =
            find_backup(&directory.0, &restored.emergency.id).expect("emergency backup");
        validate_and_extract_path(&emergency.path).expect("valid emergency backup");
    }

    #[test]
    fn retention_keeps_seven_days_and_four_older_weeks() {
        let now = Utc::now();
        let mut backups = Vec::new();
        for day in 0..50 {
            let manifest = BackupManifest {
                format_version: BACKUP_FORMAT_VERSION,
                id: format!("backup-{day}"),
                kind: BackupKind::Daily,
                created_at: (now - chrono::Duration::days(day)).to_rfc3339(),
                app_version: "test".into(),
                schema_version: db::migrations::latest_version(),
                migration_target: None,
                files: Vec::new(),
            };
            backups.push(StoredBackup {
                path: PathBuf::from(format!("backup-{day}.zip")),
                manifest,
                size: 0,
            });
        }
        let keep = retention_keep_ids(&backups).expect("retention");
        assert_eq!(keep.len(), 11);
        for day in 0..7 {
            assert!(keep.contains(&format!("backup-{day}")));
        }
    }

    #[test]
    fn manifest_rejects_path_traversal_and_future_schema() {
        let required = |path: &str| ManifestFile {
            path: path.into(),
            size: 0,
            sha256: "0".repeat(64),
        };
        let base = BackupManifest {
            format_version: BACKUP_FORMAT_VERSION,
            id: Uuid::new_v4().to_string(),
            kind: BackupKind::Manual,
            created_at: Utc::now().to_rfc3339(),
            app_version: "test".into(),
            schema_version: db::migrations::latest_version(),
            migration_target: None,
            files: vec![
                required(DATABASE_ARCHIVE_PATH),
                required(SETTINGS_ARCHIVE_PATH),
                required(TEMPLATES_ARCHIVE_PATH),
                required("media/../../outside"),
            ],
        };
        assert!(validate_manifest(&base)
            .expect_err("path traversal")
            .to_string()
            .contains("unsafe backup path"));

        let mut future = base;
        future.files.pop();
        future.schema_version = db::migrations::latest_version() + 1;
        assert!(validate_manifest(&future)
            .expect_err("future schema")
            .to_string()
            .contains("newer than this app supports"));

        future.schema_version = -1;
        assert!(validate_manifest(&future)
            .expect_err("negative schema")
            .to_string()
            .contains("negative schema version"));

        future.schema_version = db::migrations::latest_version();
        future.format_version = 1;
        validate_manifest(&future).expect("legacy v1 backup remains supported");
        future.files.push(required("attachments/not-in-v1.txt"));
        assert!(validate_manifest(&future)
            .expect_err("v1 attachment payload")
            .to_string()
            .contains("unexpected backup payload"));
    }
}
