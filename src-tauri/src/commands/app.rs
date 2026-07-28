use crate::db::DbState;
use crate::error::AppResult;
use crate::pending_open::PendingOpen;
use serde::Serialize;
use tauri::{AppHandle, State};

#[derive(Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub data_dir: String,
}

#[tauri::command]
pub fn get_app_info(db: State<'_, DbState>) -> AppResult<AppInfo> {
    Ok(AppInfo {
        name: "SoheiDesk".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: db.data_dir.to_string_lossy().to_string(),
    })
}

/// Paths from double-click / Open With / CLI — consume once at UI start.
#[tauri::command]
pub fn take_pending_open_paths(pending: State<'_, PendingOpen>) -> AppResult<Vec<String>> {
    Ok(pending.take_all())
}

/// Quit the whole application (Simple mode Quit button).
#[tauri::command]
pub fn quit_app(app: AppHandle) -> AppResult<()> {
    app.exit(0);
    Ok(())
}
