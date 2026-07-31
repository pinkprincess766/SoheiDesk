//! External parser plugins: shell commands that take a file path and print text to stdout.

use crate::db::{with_conn, DbState};
use crate::error::{AppError, AppResult};
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

const MAX_NAME_LEN: usize = 120;
const MAX_COMMAND_LEN: usize = 4096;
const MAX_ARGS: usize = 64;
const MAX_ARG_LEN: usize = 8192;
const MAX_PLUGIN_OUTPUT: usize = 8 * 1024 * 1024;
const PLUGIN_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args_json: Option<String>,
    pub extensions_json: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct PluginInput {
    pub name: String,
    pub command: String,
    pub args: Option<Vec<String>>,
    pub extensions: Vec<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Plugin> {
    Ok(Plugin {
        id: row.get(0)?,
        name: row.get(1)?,
        command: row.get(2)?,
        args_json: row.get(3)?,
        extensions_json: row.get(4)?,
        description: row.get(5)?,
        enabled: row.get::<_, i64>(6)? == 1,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

pub fn list_plugins(db: &DbState) -> AppResult<Vec<Plugin>> {
    with_conn(db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, command, args_json, extensions_json, description, enabled, created_at, updated_at
             FROM plugins ORDER BY name",
        )?;
        let rows = stmt.query_map([], map_row)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
}

pub fn create_plugin(db: &DbState, input: PluginInput) -> AppResult<Plugin> {
    if input.name.trim().is_empty() || input.command.trim().is_empty() {
        return Err(AppError::Message("name and command required".into()));
    }
    if input.name.len() > MAX_NAME_LEN || input.command.len() > MAX_COMMAND_LEN {
        return Err(AppError::Message(
            "plugin name or command is too long".into(),
        ));
    }
    if input.extensions.is_empty() {
        return Err(AppError::Message("at least one extension required".into()));
    }
    if input.extensions.len() > MAX_ARGS {
        return Err(AppError::Message("too many plugin extensions".into()));
    }
    let args = input.args.unwrap_or_default();
    if args.len() > MAX_ARGS || args.iter().any(|arg| arg.len() > MAX_ARG_LEN) {
        return Err(AppError::Message(
            "too many or too-long plugin arguments".into(),
        ));
    }
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let args = serde_json::to_string(&args)?;
    let exts: Vec<String> = input
        .extensions
        .iter()
        .map(|e| e.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .collect();
    let exts_json = serde_json::to_string(&exts)?;
    let enabled = if input.enabled.unwrap_or(true) { 1 } else { 0 };
    with_conn(db, |conn| {
        conn.execute(
            "INSERT INTO plugins (id, name, command, args_json, extensions_json, description, enabled, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8)",
            params![
                id,
                input.name.trim(),
                input.command.trim(),
                args,
                exts_json,
                input.description,
                enabled,
                now
            ],
        )?;
        Ok(())
    })?;
    get_plugin(db, &id)
}

pub fn get_plugin(db: &DbState, id: &str) -> AppResult<Plugin> {
    with_conn(db, |conn| {
        conn.query_row(
            "SELECT id, name, command, args_json, extensions_json, description, enabled, created_at, updated_at
             FROM plugins WHERE id = ?1",
            params![id],
            map_row,
        )
        .map_err(|_| AppError::Message("plugin not found".into()))
    })
}

pub fn delete_plugin(db: &DbState, id: &str) -> AppResult<()> {
    with_conn(db, |conn| {
        conn.execute("DELETE FROM plugins WHERE id = ?1", params![id])?;
        Ok(())
    })
}

pub fn set_enabled(db: &DbState, id: &str, enabled: bool) -> AppResult<Plugin> {
    let now = Utc::now().to_rfc3339();
    with_conn(db, |conn| {
        conn.execute(
            "UPDATE plugins SET enabled = ?1, updated_at = ?2 WHERE id = ?3",
            params![if enabled { 1 } else { 0 }, now, id],
        )?;
        Ok(())
    })?;
    get_plugin(db, id)
}

/// Run plugin: substitutes `{path}` in args, or appends path if no placeholder.
pub fn run_plugin(db: &DbState, plugin_id: &str, file_path: &str) -> AppResult<String> {
    let plugin = get_plugin(db, plugin_id)?;
    if !plugin.enabled {
        return Err(AppError::Message("plugin disabled".into()));
    }
    if !Path::new(file_path).is_file() {
        return Err(AppError::Message(format!("file not found: {file_path}")));
    }

    let args: Vec<String> = plugin
        .args_json
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    let mut cmd = Command::new(&plugin.command);
    if args.is_empty() {
        cmd.arg(file_path);
    } else {
        let mut has_path = false;
        for a in &args {
            if a.contains("{path}") {
                has_path = true;
                cmd.arg(a.replace("{path}", file_path));
            } else {
                cmd.arg(a);
            }
        }
        if !has_path {
            cmd.arg(file_path);
        }
    }

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::Message(format!("plugin spawn failed: {e}")))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < PLUGIN_TIMEOUT => {
                thread::sleep(Duration::from_millis(25));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::Message(
                    "plugin timed out after 30 seconds".into(),
                ));
            }
            Err(e) => {
                let _ = child.kill();
                return Err(AppError::Message(format!("plugin wait failed: {e}")));
            }
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|e| AppError::Message(format!("plugin output failed: {e}")))?;
    if output.stdout.len() > MAX_PLUGIN_OUTPUT || output.stderr.len() > MAX_PLUGIN_OUTPUT {
        return Err(AppError::Message(
            "plugin output exceeded the 8 MiB safety limit".into(),
        ));
    }
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Message(format!(
            "plugin failed ({}): {err}",
            output.status
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Find enabled plugin for file extension.
pub fn find_for_extension(db: &DbState, ext: &str) -> AppResult<Option<Plugin>> {
    let ext = ext.trim_start_matches('.').to_ascii_lowercase();
    let plugins = list_plugins(db)?;
    for p in plugins {
        if !p.enabled {
            continue;
        }
        let exts: Vec<String> = serde_json::from_str(&p.extensions_json).unwrap_or_default();
        if exts.iter().any(|e| e == &ext) {
            return Ok(Some(p));
        }
    }
    Ok(None)
}

pub fn seed_example_plugins(db: &DbState) -> AppResult<()> {
    let n: i64 = with_conn(db, |conn| {
        Ok(conn.query_row("SELECT COUNT(*) FROM plugins", [], |r| r.get(0))?)
    })?;
    if n > 0 {
        return Ok(());
    }
    // Example: pandoc as optional external converter (disabled until user enables)
    let now = Utc::now().to_rfc3339();
    with_conn(db, |conn| {
        conn.execute(
            "INSERT INTO plugins (id, name, command, args_json, extensions_json, description, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?7)",
            params![
                "plugin-pandoc-odt",
                "Pandoc ODT→Markdown (example)",
                "pandoc",
                serde_json::to_string(&vec!["{path}", "-t", "markdown"])?,
                serde_json::to_string(&vec!["odt"])?,
                "Requires pandoc in PATH. Disabled by default.",
                now
            ],
        )?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn test_state() -> (DbState, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!("soheidesk-plugin-{}.sqlite", Uuid::new_v4()));
        let conn = crate::db::open(&path).expect("test database");
        (
            DbState {
                conn: Mutex::new(conn),
                data_dir: std::env::temp_dir(),
            },
            path,
        )
    }

    fn valid_input() -> PluginInput {
        PluginInput {
            name: "Test parser".into(),
            command: "/usr/bin/false".into(),
            args: Some(vec!["--input".into(), "{path}".into()]),
            extensions: vec![".ABC".into()],
            description: None,
            enabled: Some(false),
        }
    }

    #[test]
    fn rejects_invalid_plugin_metadata() {
        let (db, path) = test_state();
        let mut input = valid_input();
        input.name.clear();
        assert!(create_plugin(&db, input).is_err());

        let mut input = valid_input();
        input.extensions.clear();
        assert!(create_plugin(&db, input).is_err());

        let mut input = valid_input();
        input.args = Some(vec!["x".into(); MAX_ARGS + 1]);
        assert!(create_plugin(&db, input).is_err());
        drop(db);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn normalizes_extensions_and_respects_enabled_state() {
        let (db, path) = test_state();
        let plugin = create_plugin(&db, valid_input()).expect("create plugin");
        assert_eq!(plugin.extensions_json, r#"["abc"]"#);
        assert!(find_for_extension(&db, "abc")
            .expect("find plugin")
            .is_none());
        set_enabled(&db, &plugin.id, true).expect("enable plugin");
        assert_eq!(
            find_for_extension(&db, ".ABC")
                .expect("find plugin")
                .expect("enabled plugin")
                .id,
            plugin.id
        );
        drop(db);
        let _ = std::fs::remove_file(path);
    }
}
