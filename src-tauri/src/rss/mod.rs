//! RSS / Atom feed subscriptions for journals.

use crate::db::{with_conn, DbState};
use crate::error::{AppError, AppResult};
use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssFeed {
    pub id: String,
    pub title: String,
    pub url: String,
    pub category: Option<String>,
    pub last_fetched_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RssItem {
    pub id: String,
    pub feed_id: String,
    pub guid: Option<String>,
    pub title: String,
    pub link: Option<String>,
    pub summary: Option<String>,
    pub published_at: Option<String>,
    pub is_read: bool,
    pub created_at: String,
}

fn map_feed(row: &rusqlite::Row<'_>) -> rusqlite::Result<RssFeed> {
    Ok(RssFeed {
        id: row.get(0)?,
        title: row.get(1)?,
        url: row.get(2)?,
        category: row.get(3)?,
        last_fetched_at: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn map_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<RssItem> {
    Ok(RssItem {
        id: row.get(0)?,
        feed_id: row.get(1)?,
        guid: row.get(2)?,
        title: row.get(3)?,
        link: row.get(4)?,
        summary: row.get(5)?,
        published_at: row.get(6)?,
        is_read: row.get::<_, i64>(7)? == 1,
        created_at: row.get(8)?,
    })
}

fn validate_feed_url(raw: &str) -> AppResult<reqwest::Url> {
    let url = reqwest::Url::parse(raw.trim())
        .map_err(|_| AppError::Message("invalid feed URL".into()))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::Message("feed URL must use HTTP or HTTPS".into()));
    }
    let host = url
        .host_str()
        .ok_or_else(|| AppError::Message("feed URL has no host".into()))?;
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") {
        return Err(AppError::Message(
            "local network feed URLs are blocked".into(),
        ));
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        let blocked = match ip {
            IpAddr::V4(ip) => {
                ip.is_private()
                    || ip.is_loopback()
                    || ip.is_link_local()
                    || ip.is_broadcast()
                    || ip.is_multicast()
                    || ip.is_unspecified()
            }
            IpAddr::V6(ip) => {
                ip.is_loopback()
                    || ip.is_unspecified()
                    || ip.is_unique_local()
                    || ip.is_unicast_link_local()
                    || ip.is_multicast()
            }
        };
        if blocked {
            return Err(AppError::Message(
                "local network feed URLs are blocked".into(),
            ));
        }
    }
    Ok(url)
}

pub fn list_feeds(db: &DbState) -> AppResult<Vec<RssFeed>> {
    with_conn(db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, url, category, last_fetched_at, created_at FROM rss_feeds ORDER BY title",
        )?;
        let rows = stmt.query_map([], map_feed)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
}

pub fn add_feed(
    db: &DbState,
    title: String,
    url: String,
    category: Option<String>,
) -> AppResult<RssFeed> {
    if url.trim().is_empty() {
        return Err(AppError::Message("feed URL required".into()));
    }
    let url = validate_feed_url(&url)?.to_string();
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let title = if title.trim().is_empty() {
        url.clone()
    } else {
        title.trim().to_string()
    };
    with_conn(db, |conn| {
        conn.execute(
            "INSERT INTO rss_feeds (id, title, url, category, last_fetched_at, created_at)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
            params![id, title, url.trim(), category, now],
        )?;
        Ok(())
    })?;
    get_feed(db, &id)
}

pub fn get_feed(db: &DbState, id: &str) -> AppResult<RssFeed> {
    with_conn(db, |conn| {
        conn.query_row(
            "SELECT id, title, url, category, last_fetched_at, created_at FROM rss_feeds WHERE id = ?1",
            params![id],
            map_feed,
        )
        .map_err(|_| AppError::Message("feed not found".into()))
    })
}

pub fn delete_feed(db: &DbState, id: &str) -> AppResult<()> {
    with_conn(db, |conn| {
        conn.execute("DELETE FROM rss_feeds WHERE id = ?1", params![id])?;
        Ok(())
    })
}

pub fn list_items(db: &DbState, feed_id: Option<String>, limit: usize) -> AppResult<Vec<RssItem>> {
    with_conn(db, |conn| {
        let limit = limit.clamp(1, 200) as i64;
        if let Some(fid) = feed_id {
            let mut stmt = conn.prepare(
                "SELECT id, feed_id, guid, title, link, summary, published_at, is_read, created_at
                 FROM rss_items WHERE feed_id = ?1
                 ORDER BY COALESCE(published_at, created_at) DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![fid, limit], map_item)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, feed_id, guid, title, link, summary, published_at, is_read, created_at
                 FROM rss_items
                 ORDER BY COALESCE(published_at, created_at) DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![limit], map_item)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        }
    })
}

pub fn mark_read(db: &DbState, id: &str, is_read: bool) -> AppResult<()> {
    with_conn(db, |conn| {
        conn.execute(
            "UPDATE rss_items SET is_read = ?1 WHERE id = ?2",
            params![if is_read { 1 } else { 0 }, id],
        )?;
        Ok(())
    })
}

#[derive(Debug)]
struct ParsedItem {
    guid: String,
    title: String,
    link: Option<String>,
    summary: Option<String>,
    published: Option<String>,
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    // handle <tag ...>content</tag> and CDATA
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let start = xml.find(&open)?;
    let after = &xml[start..];
    let gt = after.find('>')?;
    let content = &after[gt + 1..];
    let end = content.find(&close)?;
    let mut inner = content[..end].trim().to_string();
    if let Some(rest) = inner.strip_prefix("<![CDATA[") {
        if let Some(body) = rest.strip_suffix("]]>") {
            inner = body.trim().to_string();
        }
    }
    // strip nested tags lightly
    let re = regex::Regex::new(r"<[^>]+>").ok()?;
    Some(re.replace_all(&inner, "").trim().to_string())
}

fn parse_feed_xml(xml: &str) -> Vec<ParsedItem> {
    let mut items = Vec::new();
    // RSS 2.0 <item>
    for part in xml.split("<item").skip(1) {
        let body = part.split("</item>").next().unwrap_or("");
        let title = extract_tag(body, "title").unwrap_or_else(|| "(untitled)".into());
        let link = extract_tag(body, "link");
        let guid = extract_tag(body, "guid")
            .or_else(|| link.clone())
            .unwrap_or_else(|| title.clone());
        let summary = extract_tag(body, "description").or_else(|| extract_tag(body, "summary"));
        let published = extract_tag(body, "pubDate")
            .or_else(|| extract_tag(body, "published"))
            .or_else(|| extract_tag(body, "dc:date"));
        items.push(ParsedItem {
            guid,
            title,
            link,
            summary,
            published,
        });
    }
    // Atom <entry>
    if items.is_empty() {
        for part in xml.split("<entry").skip(1) {
            let body = part.split("</entry>").next().unwrap_or("");
            let title = extract_tag(body, "title").unwrap_or_else(|| "(untitled)".into());
            // link href=
            let link = body
                .find("href=\"")
                .and_then(|i| {
                    let s = &body[i + 6..];
                    s.find('"').map(|e| s[..e].to_string())
                })
                .or_else(|| extract_tag(body, "link"));
            let guid = extract_tag(body, "id")
                .or_else(|| link.clone())
                .unwrap_or_else(|| title.clone());
            let summary = extract_tag(body, "summary").or_else(|| extract_tag(body, "content"));
            let published = extract_tag(body, "published").or_else(|| extract_tag(body, "updated"));
            items.push(ParsedItem {
                guid,
                title,
                link,
                summary,
                published,
            });
        }
    }
    items
}

pub fn fetch_feed(db: &DbState, feed_id: &str) -> AppResult<usize> {
    let feed = get_feed(db, feed_id)?;
    let client = reqwest::blocking::Client::builder()
        .user_agent("SoheiDesk/0.4 RSS reader")
        .timeout(std::time::Duration::from_secs(25))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| AppError::Message(format!("http: {e}")))?;
    let text = client
        .get(validate_feed_url(&feed.url)?)
        .send()
        .map_err(|e| AppError::Message(format!("fetch feed: {e}")))?
        .text()
        .map_err(|e| AppError::Message(format!("feed body: {e}")))?;

    let parsed = parse_feed_xml(&text);
    let now = Utc::now().to_rfc3339();
    let mut inserted = 0usize;

    with_conn(db, |conn| {
        for p in &parsed {
            let id = Uuid::new_v4().to_string();
            let res = conn.execute(
                "INSERT OR IGNORE INTO rss_items
                 (id, feed_id, guid, title, link, summary, published_at, is_read, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8)",
                params![
                    id,
                    feed_id,
                    p.guid,
                    p.title,
                    p.link,
                    p.summary,
                    p.published,
                    now
                ],
            )?;
            inserted += res;
        }
        conn.execute(
            "UPDATE rss_feeds SET last_fetched_at = ?1 WHERE id = ?2",
            params![now, feed_id],
        )?;
        Ok(())
    })?;

    // auto-title from feed if still url-like
    if feed.title == feed.url {
        if let Some(t) = extract_tag(&text, "title") {
            let _ = with_conn(db, |conn| {
                conn.execute(
                    "UPDATE rss_feeds SET title = ?1 WHERE id = ?2",
                    params![t, feed_id],
                )?;
                Ok(())
            });
        }
    }

    Ok(inserted)
}

pub fn fetch_all(db: &DbState) -> AppResult<usize> {
    let feeds = list_feeds(db)?;
    let mut total = 0;
    for f in feeds {
        match fetch_feed(db, &f.id) {
            Ok(n) => total += n,
            Err(_) => continue,
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_url_rejects_local_and_non_http_targets() {
        assert!(validate_feed_url("file:///etc/passwd").is_err());
        assert!(validate_feed_url("http://localhost:8080/feed").is_err());
        assert!(validate_feed_url("http://127.0.0.1/feed").is_err());
        assert!(validate_feed_url("http://192.168.1.4/feed").is_err());
        assert!(validate_feed_url("https://example.com/feed.xml").is_ok());
    }

    #[test]
    fn parse_rss_item() {
        let xml = r#"<?xml version="1.0"?>
        <rss><channel>
          <item>
            <title>Hello Paper</title>
            <link>https://example.com/1</link>
            <guid>g1</guid>
            <description>Summary here</description>
            <pubDate>Mon, 01 Jan 2024 00:00:00 GMT</pubDate>
          </item>
        </channel></rss>"#;
        let items = parse_feed_xml(xml);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Hello Paper");
        assert_eq!(items[0].guid, "g1");
    }
}
