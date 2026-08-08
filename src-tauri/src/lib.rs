mod annotations;
mod atomic_file;
mod backup;
mod collab;
mod commands;
mod db;
mod diagnostics;
mod documents;
mod error;
mod export;
mod integrations;
mod journal;
mod library;
mod literature;
mod ocr;
mod parsers;
mod pending_open;
mod plugins;
mod portability;
mod rss;
mod search;
mod templates;

use backup::BackupState;
use collab::CollabState;
use diagnostics::DiagnosticState;
use pending_open::PendingOpen;
use portability::PortabilityState;
use search::SearchState;
#[cfg(target_os = "macos")]
use tauri::Emitter;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let pending = PendingOpen::default();
    pending.push_many(pending_open::paths_from_env_args());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(pending)
        .setup(|app| {
            let state = db::init(app.handle())?;
            let diagnostics = DiagnosticState::new(&state.data_dir)?;
            templates::seed_builtins(&state)?;
            export::seed_export_templates(&state)?;
            let _ = plugins::seed_example_plugins(&state);
            let search = match SearchState::open(&state.data_dir) {
                Ok(search) => search,
                Err(error) => {
                    let _ = diagnostics.record_error("search.startup", &error.to_string());
                    return Err(error.into());
                }
            };
            // Tantivy indices are disposable and may need rebuilding after an
            // index-format upgrade or recovery from local corruption.
            if search.needs_reindex() {
                search.reindex_all(&state)?;
            }
            app.manage(CollabState::default());
            app.manage(search);
            app.manage(state);
            app.manage(diagnostics);
            app.manage(BackupState::default());
            app.manage(PortabilityState::default());
            backup::start_scheduler(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::take_pending_open_paths,
            commands::quit_app,
            commands::get_application_diagnostics,
            commands::record_diagnostic_error,
            commands::export_diagnostic_archive,
            commands::get_setting,
            commands::set_setting,
            commands::delete_setting,
            commands::create_backup,
            commands::list_backups,
            commands::restore_backup,
            commands::export_workspace,
            commands::preview_workspace_import,
            commands::import_workspace,
            commands::open_document_path,
            commands::list_documents,
            commands::list_document_versions,
            commands::remove_document,
            commands::reopen_document,
            commands::read_authorized_file,
            commands::list_templates,
            commands::create_template,
            commands::update_template,
            commands::delete_template,
            commands::list_journal_entries,
            commands::get_journal_entry,
            commands::get_journal_draft,
            commands::list_journal_drafts,
            commands::save_journal_draft,
            commands::delete_journal_draft,
            commands::create_journal_entry,
            commands::update_journal_entry,
            commands::delete_journal_entry,
            commands::preview_journal_export,
            commands::export_journal_entry,
            commands::save_entry_as_template,
            commands::export_template_file,
            commands::import_template_file,
            commands::list_annotations,
            commands::create_annotation,
            commands::update_annotation,
            commands::delete_annotation,
            commands::export_annotations_markdown,
            commands::export_annotations_to_file,
            commands::open_in_chroma,
            commands::search_all,
            commands::reindex_all,
            commands::index_document,
            commands::zotero_list_items,
            commands::zotero_import_paths,
            commands::zotero_save_db_path,
            commands::list_export_templates,
            commands::create_export_template,
            commands::delete_export_template,
            commands::preview_entry_export,
            commands::export_entry_formatted,
            commands::preview_period_export,
            commands::export_period_formatted,
            commands::resolve_doi,
            commands::search_arxiv,
            commands::search_pubmed,
            commands::save_literature_hit,
            commands::list_bibliography,
            commands::delete_bibliography_item,
            commands::export_bibliography,
            commands::export_bibliography_to_file,
            commands::ocr_status,
            commands::ocr_image,
            commands::rss_list_feeds,
            commands::rss_add_feed,
            commands::rss_delete_feed,
            commands::rss_fetch_feed,
            commands::rss_fetch_all,
            commands::rss_list_items,
            commands::rss_mark_read,
            commands::collab_status,
            commands::collab_start,
            commands::collab_stop,
            commands::list_plugins,
            commands::create_plugin,
            commands::delete_plugin,
            commands::set_plugin_enabled,
            commands::run_plugin,
            commands::find_plugin_for_ext,
        ])
        .build(tauri::generate_context!())
        .expect("error while building SoheiDesk")
        .run(|app_handle, event| {
            // macOS: double-click / Open With while app is running or at launch
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Opened { urls } = event {
                let paths: Vec<String> = urls
                    .iter()
                    .filter_map(|u| u.to_file_path().ok())
                    .filter(|p| pending_open::is_supported_path(p))
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();
                if paths.is_empty() {
                    return;
                }
                if let Some(pending) = app_handle.try_state::<PendingOpen>() {
                    pending.push_many(paths.clone());
                }
                let _ = app_handle.emit("open-paths", paths);
            }
            let _ = app_handle;
            let _ = event;
        });
}
