use crate::db::DbState;
use crate::diagnostics::{
    self, DiagnosticArchiveResult, DiagnosticReport, DiagnosticState, PdfWorkerProbe,
};
use crate::error::AppResult;
use std::path::Path;
use tauri::State;

// Directory sizing and tool probes may touch many files or start a process, so
// Tauri must not execute this synchronous work on the webview's main thread.
#[tauri::command(async)]
pub fn get_application_diagnostics(
    db: State<'_, DbState>,
    diagnostics: State<'_, DiagnosticState>,
) -> AppResult<DiagnosticReport> {
    diagnostics::collect_report(&db, &diagnostics, None)
}

#[tauri::command]
pub fn record_diagnostic_error(
    diagnostics: State<'_, DiagnosticState>,
    category: String,
    message: String,
) -> AppResult<()> {
    diagnostics.record_error(&category, &message)
}

// ZIP creation, fsync, and validation are intentionally performed away from
// the main thread while retaining the existing synchronous storage contract.
#[tauri::command(async)]
pub fn export_diagnostic_archive(
    db: State<'_, DbState>,
    diagnostics: State<'_, DiagnosticState>,
    path: String,
    pdf_worker: Option<PdfWorkerProbe>,
) -> AppResult<DiagnosticArchiveResult> {
    diagnostics::export_archive(&db, &diagnostics, Path::new(&path), pdf_worker)
}
