mod annotations;
mod collab;
mod commands;
mod db;
mod documents;
mod error;
mod export;
mod integrations;
mod journal;
mod library;
mod literature;
mod ocr;
mod parsers;
mod plugins;
mod rss;
mod search;
mod templates;

use collab::CollabState;
use search::SearchState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let state = db::init(app.handle())?;
            templates::seed_builtins(&state)?;
            export::seed_export_templates(&state)?;
            let _ = plugins::seed_example_plugins(&state);
            let search = SearchState::open(&state.data_dir)?;
            let _ = search.reindex_all(&state);
            app.manage(CollabState::default());
            app.manage(search);
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::get_setting,
            commands::set_setting,
            commands::open_document_path,
            commands::list_documents,
            commands::remove_document,
            commands::reopen_document,
            commands::read_authorized_file,
            commands::list_templates,
            commands::create_template,
            commands::update_template,
            commands::delete_template,
            commands::list_journal_entries,
            commands::get_journal_entry,
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
            // stage 4
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
        .run(tauri::generate_context!())
        .expect("error while running SoheiDesk");
}
