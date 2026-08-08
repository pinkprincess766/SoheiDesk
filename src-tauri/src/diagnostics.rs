//! Privacy-safe local diagnostics and support archives.
//!
//! Diagnostic output deliberately contains operational metadata only. Raw
//! errors, document contents, settings, URLs, and filesystem paths are never
//! persisted because support data is commonly shared outside the workstation.

use crate::atomic_file;
use crate::backup::{self, BackupKind};
use crate::db::{self, with_conn, DbState};
use crate::error::{AppError, AppResult};
use crate::{ocr, parsers};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const DIAGNOSTIC_FORMAT_VERSION: u32 = 1;
const LOG_FILE_NAME: &str = "soheidesk.log";
const LOG_MAX_BYTES: u64 = 512 * 1024;
const LOG_ROTATIONS: usize = 4;
const MAX_RECENT_EVENTS: usize = 50;
const MAX_ARCHIVE_ENTRY_BYTES: u64 = 2 * 1024 * 1024;
const REPORT_ARCHIVE_PATH: &str = "diagnostics.json";
const ERRORS_ARCHIVE_PATH: &str = "errors.jsonl";
const README_ARCHIVE_PATH: &str = "README.txt";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComponentState {
    Available,
    Unavailable,
    NotConfigured,
    NotChecked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentStatus {
    pub state: ComponentState,
    pub version: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfWorkerProbe {
    pub state: ComponentState,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticEvent {
    pub timestamp: String,
    pub level: String,
    pub category: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityStatus {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageMetric {
    pub bytes: u64,
    pub files: u64,
    pub accessible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStatus {
    pub database: StorageMetric,
    pub attachments: StorageMetric,
    pub media: StorageMetric,
    pub search_index: StorageMetric,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSummary {
    pub kind: BackupKind,
    pub created_at: String,
    pub size_bytes: u64,
    pub schema_version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentStatuses {
    pub pdf_worker: ComponentStatus,
    pub ocr: ComponentStatus,
    pub djvu: ComponentStatus,
    pub chroma_tsvet: ComponentStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticReport {
    pub format_version: u32,
    pub generated_at: String,
    pub app_version: String,
    pub database_schema_version: Option<i64>,
    pub supported_schema_version: i64,
    pub integrity: IntegrityStatus,
    pub last_successful_backup: Option<BackupSummary>,
    pub storage: StorageStatus,
    pub components: ComponentStatuses,
    pub recent_errors: Vec<DiagnosticEvent>,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticArchiveResult {
    pub file_name: String,
    pub size_bytes: u64,
    pub generated_at: String,
}

pub struct DiagnosticState {
    log: Mutex<DiagnosticLog>,
}

impl DiagnosticState {
    pub fn new(data_dir: &Path) -> AppResult<Self> {
        Ok(Self {
            log: Mutex::new(DiagnosticLog::new(data_dir)?),
        })
    }

    /// Classifies a raw error and persists only a fixed, content-free summary.
    pub fn record_error(&self, category: &str, raw_message: &str) -> AppResult<()> {
        let event = DiagnosticEvent {
            timestamp: Utc::now().to_rfc3339(),
            level: "error".into(),
            category: classify_category(category).into(),
            message: classify_error(raw_message).into(),
        };
        self.log
            .lock()
            .map_err(|_| AppError::Message("diagnostic log lock poisoned".into()))?
            .append(&event)
    }

    fn recent_errors(&self) -> AppResult<Vec<DiagnosticEvent>> {
        self.log
            .lock()
            .map_err(|_| AppError::Message("diagnostic log lock poisoned".into()))?
            .recent_events()
    }
}

struct DiagnosticLog {
    directory: PathBuf,
    current: PathBuf,
}

impl DiagnosticLog {
    fn new(data_dir: &Path) -> AppResult<Self> {
        let diagnostics_root = data_dir.join("diagnostics");
        ensure_private_directory(&diagnostics_root)?;
        let directory = diagnostics_root.join("logs");
        ensure_private_directory(&directory)?;
        Ok(Self {
            current: directory.join(LOG_FILE_NAME),
            directory,
        })
    }

    fn rotated(&self, index: usize) -> PathBuf {
        self.directory.join(format!("{LOG_FILE_NAME}.{index}"))
    }

    fn append(&mut self, event: &DiagnosticEvent) -> AppResult<()> {
        let mut encoded = serde_json::to_vec(event)?;
        encoded.push(b'\n');
        self.rotate_if_needed(encoded.len() as u64)?;
        refuse_symlink(&self.current)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.current)?;
        // Re-apply the private mode on every open. An older build or a manual
        // permission change must not leave future diagnostic events readable
        // by other local accounts.
        set_private_file_permissions(&file)?;
        file.write_all(&encoded)?;
        file.sync_data()?;
        Ok(())
    }

    fn rotate_if_needed(&self, incoming: u64) -> AppResult<()> {
        let current_size = match fs::symlink_metadata(&self.current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(AppError::Message(
                    "diagnostic log path must not be a symlink".into(),
                ));
            }
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error.into()),
        };
        if current_size.saturating_add(incoming) <= LOG_MAX_BYTES {
            return Ok(());
        }

        let oldest = self.rotated(LOG_ROTATIONS);
        remove_regular_file_if_present(&oldest)?;
        for index in (1..LOG_ROTATIONS).rev() {
            let source = self.rotated(index);
            let destination = self.rotated(index + 1);
            rename_regular_file_if_present(&source, &destination)?;
        }
        rename_regular_file_if_present(&self.current, &self.rotated(1))?;
        Ok(())
    }

    fn recent_events(&self) -> AppResult<Vec<DiagnosticEvent>> {
        let mut events = Vec::new();
        for index in (1..=LOG_ROTATIONS).rev() {
            read_log_file(&self.rotated(index), &mut events)?;
        }
        read_log_file(&self.current, &mut events)?;
        if events.len() > MAX_RECENT_EVENTS {
            events.drain(..events.len() - MAX_RECENT_EVENTS);
        }
        Ok(events)
    }
}

fn ensure_private_directory(path: &Path) -> AppResult<()> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(AppError::Message(
                "diagnostic log directory is not a private directory".into(),
            ));
        }
        set_private_directory_permissions(path)?;
        return Ok(());
    }
    fs::create_dir_all(path)?;
    set_private_directory_permissions(path)?;
    Ok(())
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> AppResult<()> {
    // Windows access control is inherited from the application-data directory.
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(file: &File) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_file_permissions(_file: &File) -> AppResult<()> {
    Ok(())
}

fn refuse_symlink(path: &Path) -> AppResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(AppError::Message(
            "diagnostic log path must not be a symlink".into(),
        )),
        Ok(metadata) if !metadata.is_file() => Err(AppError::Message(
            "diagnostic log path is not a regular file".into(),
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn remove_regular_file_if_present(path: &Path) -> AppResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => Err(
            AppError::Message("refusing unsafe diagnostic log rotation target".into()),
        ),
        Ok(_) => {
            fs::remove_file(path)?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn rename_regular_file_if_present(source: &Path, destination: &Path) -> AppResult<()> {
    match fs::symlink_metadata(source) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => Err(
            AppError::Message("refusing unsafe diagnostic log rotation source".into()),
        ),
        Ok(_) => {
            remove_regular_file_if_present(destination)?;
            fs::rename(source, destination)?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn read_log_file(path: &Path, events: &mut Vec<DiagnosticEvent>) -> AppResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(AppError::Message(
                "refusing unsafe diagnostic log file".into(),
            ));
        }
        Ok(metadata) if metadata.len() > LOG_MAX_BYTES + 4096 => {
            return Err(AppError::Message(
                "diagnostic log exceeds rotation limit".into(),
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    }

    let reader = BufReader::new(File::open(path)?);
    for line in reader.lines() {
        let line = line?;
        if line.len() > 2048 {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<DiagnosticEvent>(&line) {
            events.push(event);
        }
    }
    Ok(())
}

fn classify_category(category: &str) -> &'static str {
    let category = category.to_ascii_lowercase();
    if category.starts_with("app") {
        "application"
    } else if category.starts_with("database") || category.starts_with("migration") {
        "database"
    } else if category.starts_with("backup") {
        "backup"
    } else if category.starts_with("diagnostics") {
        "diagnostics"
    } else if category.starts_with("export") {
        "export"
    } else if category.starts_with("import") || category.starts_with("portability") {
        "import"
    } else if category.starts_with("journal") {
        "journal"
    } else if category.starts_with("library") || category.starts_with("document") {
        "library"
    } else if category.starts_with("ocr") {
        "ocr"
    } else if category.starts_with("pdf") || category.starts_with("reader") {
        "pdf"
    } else if category.starts_with("plugin") {
        "plugins"
    } else if category.starts_with("rss") || category.starts_with("literature") {
        "network"
    } else if category.starts_with("search") {
        "search"
    } else if category.starts_with("settings") {
        "settings"
    } else {
        "frontend"
    }
}

/// Maps raw text to a finite vocabulary. This is intentionally lossy: a
/// timestamp and category are useful for support, while article text and paths
/// are never worth the privacy risk of retaining arbitrary error strings.
fn classify_error(raw: &str) -> &'static str {
    let message = raw.to_ascii_lowercase();
    if message.contains("migration") || message.contains("schema") {
        "Database migration failed"
    } else if message.contains("sqlite") || message.contains("database") {
        "Database operation failed"
    } else if message.contains("backup") || message.contains("резервн") {
        "Backup operation failed"
    } else if message.contains("permission")
        || message.contains("access denied")
        || message.contains("read-only")
        || message.contains("readonly")
    {
        "Storage permission denied"
    } else if message.contains("disk")
        || message.contains("no space")
        || message.contains("write zero")
    {
        "Storage write failed"
    } else if message.contains("pdf") {
        "PDF operation failed"
    } else if message.contains("tesseract") || message.contains("ocr") {
        "OCR operation failed"
    } else if message.contains("djvu") {
        "DjVu operation failed"
    } else if message.contains("rss")
        || message.contains("http")
        || message.contains("network")
        || message.contains("request")
        || message.contains("pubmed")
        || message.contains("arxiv")
    {
        "Network operation failed"
    } else if message.contains("import") {
        "Import failed"
    } else if message.contains("export") {
        "Export failed"
    } else if message.contains("plugin") {
        "Plugin operation failed"
    } else if message.contains("annotation") {
        "Annotation operation failed"
    } else if message.contains("document") || message.contains("file") || message.contains("path") {
        "Document operation failed"
    } else {
        "Unexpected application error"
    }
}

fn fixed_component(
    state: ComponentState,
    version: Option<String>,
    message: &str,
) -> ComponentStatus {
    ComponentStatus {
        state,
        version: version.and_then(safe_version),
        message: message.into(),
    }
}

fn safe_version(value: String) -> Option<String> {
    value.split_whitespace().find_map(|token| {
        let token =
            token.trim_matches(|character: char| matches!(character, ',' | ';' | '(' | ')'));
        let starts_with_digit = token
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit());
        let is_version = starts_with_digit
            && token.len() <= 32
            && token.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+' | '_')
            });
        is_version.then(|| token.to_string())
    })
}

fn pdf_worker_component(probe: Option<PdfWorkerProbe>) -> ComponentStatus {
    match probe {
        Some(PdfWorkerProbe {
            state: ComponentState::Available,
            version,
        }) => fixed_component(
            ComponentState::Available,
            version,
            "PDF worker completed its browser handshake.",
        ),
        Some(PdfWorkerProbe {
            state: ComponentState::Unavailable,
            ..
        }) => fixed_component(
            ComponentState::Unavailable,
            None,
            "PDF worker browser handshake failed.",
        ),
        _ => fixed_component(
            ComponentState::NotChecked,
            None,
            "PDF worker must be checked by the application webview.",
        ),
    }
}

fn ocr_component() -> ComponentStatus {
    let status = ocr::tesseract_status();
    if status.available {
        fixed_component(
            ComponentState::Available,
            status.version,
            "Tesseract is available.",
        )
    } else {
        fixed_component(
            ComponentState::Unavailable,
            None,
            "Tesseract was not found.",
        )
    }
}

fn djvu_component() -> ComponentStatus {
    if parsers::djvu::tool_available() {
        fixed_component(
            ComponentState::Available,
            None,
            "DjVuLibre text extraction is available.",
        )
    } else {
        fixed_component(
            ComponentState::Unavailable,
            None,
            "DjVuLibre text extraction was not found.",
        )
    }
}

fn chroma_component(db: &DbState) -> ComponentStatus {
    let configured = with_conn(db, |connection| {
        let value = connection.query_row(
            "SELECT value FROM settings WHERE key = 'chroma_path'",
            [],
            |row| row.get::<_, String>(0),
        );
        match value {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    });

    match configured {
        Ok(Some(path)) if !path.trim().is_empty() && Path::new(&path).is_file() => fixed_component(
            ComponentState::Available,
            None,
            "Configured ChromaTsvet executable is present.",
        ),
        Ok(Some(path)) if !path.trim().is_empty() => fixed_component(
            ComponentState::Unavailable,
            None,
            "Configured ChromaTsvet executable is missing.",
        ),
        Ok(_) => fixed_component(
            ComponentState::NotConfigured,
            None,
            "ChromaTsvet integration is not configured.",
        ),
        Err(_) => fixed_component(
            ComponentState::Unavailable,
            None,
            "ChromaTsvet configuration could not be checked.",
        ),
    }
}

fn database_status(db: &DbState) -> (Option<i64>, IntegrityStatus) {
    let result = with_conn(db, |connection| {
        let version = db::migrations::current_version(connection)?;
        let check: String = connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
        Ok((version, check == "ok"))
    });
    match result {
        Ok((version, true)) => (
            Some(version),
            IntegrityStatus {
                ok: true,
                message: "SQLite quick_check passed.".into(),
            },
        ),
        Ok((version, false)) => (
            Some(version),
            IntegrityStatus {
                ok: false,
                message: "SQLite quick_check reported corruption.".into(),
            },
        ),
        Err(_) => (
            None,
            IntegrityStatus {
                ok: false,
                message: "SQLite quick_check could not be completed.".into(),
            },
        ),
    }
}

fn latest_successful_backup(data_dir: &Path) -> Option<BackupSummary> {
    backup::list_backups(data_dir)
        .ok()?
        .into_iter()
        .find(|item| item.readable)
        .map(|item| BackupSummary {
            kind: item.kind,
            created_at: item.created_at,
            size_bytes: item.size_bytes,
            schema_version: item.schema_version,
        })
}

fn file_metric(paths: &[PathBuf]) -> StorageMetric {
    let mut metric = StorageMetric {
        bytes: 0,
        files: 0,
        accessible: true,
    };
    for path in paths {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                metric.accessible = false;
            }
            Ok(metadata) => {
                metric.bytes = metric.bytes.saturating_add(metadata.len());
                metric.files = metric.files.saturating_add(1);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => metric.accessible = false,
        }
    }
    metric
}

fn tree_metric(root: &Path) -> StorageMetric {
    if !root.exists() {
        return StorageMetric {
            bytes: 0,
            files: 0,
            accessible: true,
        };
    }
    let mut metric = StorageMetric {
        bytes: 0,
        files: 0,
        accessible: true,
    };
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => {
                metric.accessible = false;
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => {
                    metric.accessible = false;
                    continue;
                }
            };
            let metadata = match entry.file_type() {
                Ok(file_type) if file_type.is_symlink() => {
                    metric.accessible = false;
                    continue;
                }
                Ok(file_type) if file_type.is_dir() => {
                    pending.push(entry.path());
                    continue;
                }
                Ok(file_type) if file_type.is_file() => match entry.metadata() {
                    Ok(metadata) => metadata,
                    Err(_) => {
                        metric.accessible = false;
                        continue;
                    }
                },
                Ok(_) => continue,
                Err(_) => {
                    metric.accessible = false;
                    continue;
                }
            };
            metric.bytes = metric.bytes.saturating_add(metadata.len());
            metric.files = metric.files.saturating_add(1);
        }
    }
    metric
}

fn storage_status(data_dir: &Path) -> StorageStatus {
    let database = data_dir.join("soheidesk.sqlite");
    StorageStatus {
        database: file_metric(&[
            database.clone(),
            PathBuf::from(format!("{}-wal", database.to_string_lossy())),
            PathBuf::from(format!("{}-shm", database.to_string_lossy())),
        ]),
        attachments: tree_metric(&data_dir.join("attachments")),
        media: tree_metric(&data_dir.join("media")),
        search_index: tree_metric(&data_dir.join("tantivy_index")),
    }
}

pub fn collect_report(
    db: &DbState,
    diagnostics: &DiagnosticState,
    pdf_probe: Option<PdfWorkerProbe>,
) -> AppResult<DiagnosticReport> {
    let (database_schema_version, integrity) = database_status(db);
    Ok(DiagnosticReport {
        format_version: DIAGNOSTIC_FORMAT_VERSION,
        generated_at: Utc::now().to_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        database_schema_version,
        supported_schema_version: db::migrations::latest_version(),
        integrity,
        last_successful_backup: latest_successful_backup(&db.data_dir),
        storage: storage_status(&db.data_dir),
        components: ComponentStatuses {
            pdf_worker: pdf_worker_component(pdf_probe),
            ocr: ocr_component(),
            djvu: djvu_component(),
            chroma_tsvet: chroma_component(db),
        },
        recent_errors: diagnostics.recent_errors()?,
    })
}

fn ensure_export_outside_app_data(destination: &Path, data_dir: &Path) -> AppResult<()> {
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent)?;
    let data_dir = fs::canonicalize(data_dir)?;
    if parent.starts_with(&data_dir) {
        return Err(AppError::Message(
            "diagnostic archive must be saved outside application data".into(),
        ));
    }
    Ok(())
}

pub fn export_archive(
    db: &DbState,
    diagnostics: &DiagnosticState,
    destination: &Path,
    pdf_probe: Option<PdfWorkerProbe>,
) -> AppResult<DiagnosticArchiveResult> {
    ensure_export_outside_app_data(destination, &db.data_dir)?;
    let report = collect_report(db, diagnostics, pdf_probe)?;
    let report_json = serde_json::to_vec_pretty(&report)?;
    let mut errors_jsonl = Vec::new();
    for event in &report.recent_errors {
        serde_json::to_writer(&mut errors_jsonl, event)?;
        errors_jsonl.push(b'\n');
    }
    let readme = b"SoheiDesk diagnostic archive\n\nThis archive contains operational metadata only.\nIt does not contain the database, documents, annotations, notes, settings, URLs, configured filesystem paths, or secrets.\n";

    atomic_file::write_file(
        destination,
        |file| {
            // New support archives contain no user content, but they are still
            // private by default because they describe the local installation.
            set_private_file_permissions(&file)?;
            let options = SimpleFileOptions::default()
                .compression_method(CompressionMethod::Deflated)
                .unix_permissions(0o600);
            let mut archive = ZipWriter::new(file);
            archive
                .start_file(REPORT_ARCHIVE_PATH, options)
                .map_err(|error| zip_error("start diagnostic report", error))?;
            archive.write_all(&report_json)?;
            archive
                .start_file(ERRORS_ARCHIVE_PATH, options)
                .map_err(|error| zip_error("start diagnostic errors", error))?;
            archive.write_all(&errors_jsonl)?;
            archive
                .start_file(README_ARCHIVE_PATH, options)
                .map_err(|error| zip_error("start diagnostic README", error))?;
            archive.write_all(readme)?;
            archive
                .finish()
                .map_err(|error| zip_error("finish diagnostic archive", error))
        },
        validate_archive,
    )?;

    let size_bytes = fs::metadata(destination)?.len();
    Ok(DiagnosticArchiveResult {
        file_name: destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("soheidesk-diagnostics.zip")
            .to_string(),
        size_bytes,
        generated_at: report.generated_at,
    })
}

fn zip_error(context: &str, error: zip::result::ZipError) -> AppError {
    AppError::Message(format!("{context}: {error}"))
}

fn validate_archive(path: &Path) -> AppResult<()> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| AppError::Message(format!("open diagnostic archive: {error}")))?;
    if archive.len() != 3 {
        return Err(AppError::Message(
            "diagnostic archive has unexpected entries".into(),
        ));
    }
    let mut names = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| {
            AppError::Message(format!("read diagnostic archive entry: {error}"))
        })?;
        if entry.size() > MAX_ARCHIVE_ENTRY_BYTES {
            return Err(AppError::Message(
                "diagnostic archive entry is too large".into(),
            ));
        }
        names.push(entry.name().to_string());
    }
    names.sort();
    let mut expected = vec![
        REPORT_ARCHIVE_PATH.to_string(),
        ERRORS_ARCHIVE_PATH.to_string(),
        README_ARCHIVE_PATH.to_string(),
    ];
    expected.sort();
    if names != expected {
        return Err(AppError::Message(
            "diagnostic archive layout is invalid".into(),
        ));
    }

    let mut report = String::new();
    archive
        .by_name(REPORT_ARCHIVE_PATH)
        .map_err(|error| AppError::Message(format!("read diagnostic report: {error}")))?
        .read_to_string(&mut report)?;
    let parsed: DiagnosticReport = serde_json::from_str(&report)?;
    if parsed.format_version != DIAGNOSTIC_FORMAT_VERSION {
        return Err(AppError::Message(
            "diagnostic archive version is invalid".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use uuid::Uuid;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "soheidesk-diagnostics-test-{}",
                Uuid::new_v4().simple()
            ));
            fs::create_dir(&path).expect("create test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn test_state(directory: &TestDirectory) -> DbState {
        let database = directory.0.join("soheidesk.sqlite");
        let connection = db::open(&database).expect("test database");
        DbState {
            conn: Mutex::new(connection),
            media: Mutex::new(()),
            data_dir: directory.0.clone(),
        }
    }

    #[test]
    fn raw_content_paths_urls_and_secrets_are_never_logged() {
        let directory = TestDirectory::new();
        let diagnostics = DiagnosticState::new(&directory.0).expect("diagnostics");
        diagnostics
            .record_error(
                "Reader PRIVATE_CATEGORY",
                "PDF failed at /Users/alice/secret/article.pdf token=hunter2 note=PRIVATE_TEXT https://private.example/x",
            )
            .expect("record error");

        let events = diagnostics.recent_errors().expect("recent errors");
        let encoded = serde_json::to_string(&events).expect("events JSON");
        assert_eq!(events[0].category, "pdf");
        assert_eq!(events[0].message, "PDF operation failed");
        for forbidden in [
            "/Users/alice",
            "hunter2",
            "PRIVATE_TEXT",
            "PRIVATE_CATEGORY",
            "private.example",
        ] {
            assert!(!encoded.contains(forbidden));
        }
    }

    #[test]
    fn component_versions_keep_only_a_bounded_version_token() {
        assert_eq!(
            safe_version("tesseract 5.5.1 /Users/alice token=hunter2".into()),
            Some("5.5.1".into())
        );
        assert_eq!(safe_version("PRIVATE_OUTPUT token=hunter2".into()), None);
    }

    #[cfg(unix)]
    #[test]
    fn diagnostic_directories_and_logs_are_private() {
        use std::os::unix::fs::PermissionsExt;

        let directory = TestDirectory::new();
        let diagnostics_root = directory.0.join("diagnostics");
        let logs = diagnostics_root.join("logs");
        fs::create_dir_all(&logs).expect("legacy diagnostic directories");
        fs::set_permissions(&diagnostics_root, fs::Permissions::from_mode(0o755))
            .expect("open root permissions");
        fs::set_permissions(&logs, fs::Permissions::from_mode(0o755))
            .expect("open log permissions");

        let diagnostics = DiagnosticState::new(&directory.0).expect("diagnostics");
        diagnostics
            .record_error("frontend", "unexpected failure")
            .expect("record error");

        assert_eq!(
            fs::metadata(&diagnostics_root)
                .expect("root metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&logs)
                .expect("logs metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(logs.join(LOG_FILE_NAME))
                .expect("log metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn log_rotation_keeps_a_bounded_history() {
        let directory = TestDirectory::new();
        let mut log = DiagnosticLog::new(&directory.0).expect("log");
        fs::write(&log.current, vec![b'x'; LOG_MAX_BYTES as usize]).expect("large log");
        log.append(&DiagnosticEvent {
            timestamp: Utc::now().to_rfc3339(),
            level: "error".into(),
            category: "storage".into(),
            message: "Storage write failed".into(),
        })
        .expect("rotated append");

        assert!(log.rotated(1).is_file());
        assert!(fs::metadata(&log.current).expect("current metadata").len() < LOG_MAX_BYTES);
    }

    #[cfg(unix)]
    #[test]
    fn storage_scan_does_not_follow_symlinks() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new();
        let attachments = directory.0.join("attachments");
        fs::create_dir(&attachments).expect("attachments");
        fs::write(attachments.join("visible.bin"), vec![0_u8; 12]).expect("attachment");
        let outside = directory.0.join("outside.bin");
        fs::write(&outside, vec![0_u8; 4096]).expect("outside");
        symlink(&outside, attachments.join("linked.bin")).expect("symlink");

        let metric = tree_metric(&attachments);
        assert_eq!(metric.bytes, 12);
        assert_eq!(metric.files, 1);
        assert!(!metric.accessible);
    }

    #[test]
    fn diagnostic_archive_contains_only_allowlisted_safe_entries() {
        let app_data = TestDirectory::new();
        let export_directory = TestDirectory::new();
        let db = test_state(&app_data);
        let diagnostics = DiagnosticState::new(&app_data.0).expect("diagnostics");
        diagnostics
            .record_error(
                "reader",
                "file /Users/alice/article.pdf contained PRIVATE_NOTE token=hunter2",
            )
            .expect("record error");
        let destination = export_directory.0.join("diagnostics.zip");

        export_archive(
            &db,
            &diagnostics,
            &destination,
            Some(PdfWorkerProbe {
                state: ComponentState::Available,
                version: Some("6.1.200".into()),
            }),
        )
        .expect("export diagnostics");
        validate_archive(&destination).expect("validate diagnostics");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&destination)
                    .expect("archive metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }

        let bytes = fs::read(&destination).expect("archive bytes");
        let archive_text = String::from_utf8_lossy(&bytes);
        for forbidden in ["/Users/alice", "PRIVATE_NOTE", "hunter2"] {
            assert!(!archive_text.contains(forbidden));
        }
        let mut archive =
            ZipArchive::new(File::open(destination).expect("archive")).expect("read archive");
        let names: Vec<_> = (0..archive.len())
            .map(|index| archive.by_index(index).expect("entry").name().to_string())
            .collect();
        assert_eq!(
            names,
            vec![
                REPORT_ARCHIVE_PATH,
                ERRORS_ARCHIVE_PATH,
                README_ARCHIVE_PATH
            ]
        );
    }

    #[test]
    fn export_refuses_to_write_inside_application_data() {
        let directory = TestDirectory::new();
        let db = test_state(&directory);
        let diagnostics = DiagnosticState::new(&directory.0).expect("diagnostics");

        let error = export_archive(&db, &diagnostics, &directory.0.join("unsafe.zip"), None)
            .expect_err("app-data export must be refused");

        assert!(error.to_string().contains("outside application data"));
    }
}
