use crate::db::DbState;
use crate::error::AppResult;
use crate::rss::{self, RssFeed, RssItem};
use tauri::State;

#[tauri::command]
pub fn rss_list_feeds(db: State<'_, DbState>) -> AppResult<Vec<RssFeed>> {
    rss::list_feeds(&db)
}

#[tauri::command]
pub fn rss_add_feed(
    db: State<'_, DbState>,
    title: String,
    url: String,
    category: Option<String>,
) -> AppResult<RssFeed> {
    rss::add_feed(&db, title, url, category)
}

#[tauri::command]
pub fn rss_delete_feed(db: State<'_, DbState>, id: String) -> AppResult<()> {
    rss::delete_feed(&db, &id)
}

#[tauri::command]
pub fn rss_fetch_feed(db: State<'_, DbState>, id: String) -> AppResult<usize> {
    rss::fetch_feed(&db, &id)
}

#[tauri::command]
pub fn rss_fetch_all(db: State<'_, DbState>) -> AppResult<usize> {
    rss::fetch_all(&db)
}

#[tauri::command]
pub fn rss_list_items(
    db: State<'_, DbState>,
    feed_id: Option<String>,
    limit: Option<usize>,
) -> AppResult<Vec<RssItem>> {
    rss::list_items(&db, feed_id, limit.unwrap_or(80))
}

#[tauri::command]
pub fn rss_mark_read(db: State<'_, DbState>, id: String, is_read: bool) -> AppResult<()> {
    rss::mark_read(&db, &id, is_read)
}
