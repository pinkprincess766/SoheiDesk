use crate::db::DbState;
use crate::error::AppResult;
use serde::Serialize;
use tauri::State;

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
