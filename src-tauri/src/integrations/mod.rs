pub mod zotero;

use crate::db::{with_conn, DbState};
use crate::error::{AppError, AppResult};
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Deserialize)]
pub struct ChromaRange {
    pub from: f64,
    pub to: f64,
}

/// Soft integration: launch external ChromaTsvet binary if configured.
pub fn open_in_chroma(
    db: &DbState,
    spectrum_path: String,
    range: Option<ChromaRange>,
) -> AppResult<String> {
    let path = Path::new(&spectrum_path);
    if !path.is_file() {
        return Err(AppError::Message(format!(
            "spectrum file not found: {spectrum_path}"
        )));
    }

    let chroma = with_conn(db, |conn| {
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = 'chroma_path'")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let v: String = row.get(0)?;
            Ok(Some(v))
        } else {
            Ok(None)
        }
    })?
    .filter(|s| !s.trim().is_empty())
    .ok_or_else(|| {
        AppError::Message("ChromaTsvet path not configured. Set it in Settings.".into())
    })?;

    if !Path::new(&chroma).exists() {
        return Err(AppError::Message(format!(
            "ChromaTsvet binary not found: {chroma}"
        )));
    }

    let mut cmd = Command::new(&chroma);
    cmd.arg("--open").arg(&spectrum_path);
    if let Some(r) = range {
        cmd.arg("--from").arg(r.from.to_string());
        cmd.arg("--to").arg(r.to.to_string());
    }

    match cmd.spawn() {
        Ok(child) => Ok(format!("spawned pid {:?}", child.id())),
        Err(e) => Err(AppError::Message(format!(
            "failed to launch ChromaTsvet: {e}"
        ))),
    }
}
