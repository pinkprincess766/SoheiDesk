use crate::db::DbState;
use crate::error::AppResult;
use crate::plugins::{self, Plugin, PluginInput};
use tauri::State;

#[tauri::command]
pub fn list_plugins(db: State<'_, DbState>) -> AppResult<Vec<Plugin>> {
    plugins::list_plugins(&db)
}

#[tauri::command]
pub fn create_plugin(db: State<'_, DbState>, input: PluginInput) -> AppResult<Plugin> {
    plugins::create_plugin(&db, input)
}

#[tauri::command]
pub fn delete_plugin(db: State<'_, DbState>, id: String) -> AppResult<()> {
    plugins::delete_plugin(&db, &id)
}

#[tauri::command]
pub fn set_plugin_enabled(db: State<'_, DbState>, id: String, enabled: bool) -> AppResult<Plugin> {
    plugins::set_enabled(&db, &id, enabled)
}

#[tauri::command]
pub fn run_plugin(
    db: State<'_, DbState>,
    plugin_id: String,
    file_path: String,
) -> AppResult<String> {
    plugins::run_plugin(&db, &plugin_id, &file_path)
}

#[tauri::command]
pub fn find_plugin_for_ext(db: State<'_, DbState>, ext: String) -> AppResult<Option<Plugin>> {
    plugins::find_for_extension(&db, &ext)
}
