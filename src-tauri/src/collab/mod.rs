//! Lightweight LAN read-only share server (journal + bibliography HTML).

use crate::db::DbState;
use crate::error::{AppError, AppResult};
use parking_lot::Mutex;
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;
use tiny_http::{Header, Method, Response, Server};

#[derive(Debug, Serialize, Clone)]
pub struct CollabStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub url: Option<String>,
    pub message: String,
}

pub struct CollabState {
    inner: Mutex<Option<CollabHandle>>,
}

struct CollabHandle {
    port: u16,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

impl Default for CollabState {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }
}

impl CollabState {
    pub fn status(&self) -> CollabStatus {
        let g = self.inner.lock();
        if let Some(h) = g.as_ref() {
            CollabStatus {
                running: true,
                port: Some(h.port),
                url: Some(format!("http://127.0.0.1:{}/", h.port)),
                message: format!(
                    "LAN share running on port {}. Others on the same network can open http://<your-ip>:{}/",
                    h.port, h.port
                ),
            }
        } else {
            CollabStatus {
                running: false,
                port: None,
                url: None,
                message: "LAN share is stopped.".into(),
            }
        }
    }

    pub fn stop(&self) {
        let mut g = self.inner.lock();
        if let Some(h) = g.take() {
            h.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    pub fn start(&self, db: &DbState, port: u16) -> AppResult<CollabStatus> {
        {
            let g = self.inner.lock();
            if g.is_some() {
                return Ok(self.status());
            }
        }

        let addr: SocketAddr = format!("0.0.0.0:{port}")
            .parse()
            .map_err(|e| AppError::Message(format!("bad port: {e}")))?;
        let server = Server::http(addr).map_err(|e| AppError::Message(format!("bind failed: {e}")))?;
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_t = stop.clone();

        // Snapshot data dir path for thread — re-open SQLite readonly per request
        let db_path = db.data_dir.join("soheidesk.sqlite");

        thread::spawn(move || {
            for request in server.incoming_requests() {
                if stop_t.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                let url = request.url().to_string();
                let method = request.method().clone();
                if method != Method::Get {
                    let _ = request.respond(Response::from_string("method not allowed").with_status_code(405));
                    continue;
                }

                let body = match render_page(&db_path, &url) {
                    Ok(html) => html,
                    Err(e) => format!("<pre>error: {e}</pre>"),
                };
                let mut response = Response::from_string(body);
                if let Ok(h) = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]) {
                    response = response.with_header(h);
                }
                let _ = request.respond(response);
            }
        });

        *self.inner.lock() = Some(CollabHandle { port, stop });
        Ok(self.status())
    }
}

fn render_page(db_path: &std::path::Path, url: &str) -> AppResult<String> {
    let conn = rusqlite::Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| AppError::Message(format!("open db: {e}")))?;

    let path = url.split('?').next().unwrap_or("/");
    match path {
        "/" | "/index.html" => Ok(page_shell(
            "SoheiDesk LAN",
            r#"
            <h1>SoheiDesk · LAN share</h1>
            <p>Read-only snapshot for local network collaboration.</p>
            <ul>
              <li><a href="/journal">Journal entries</a></li>
              <li><a href="/bibliography">Bibliography</a></li>
              <li><a href="/health">Health</a></li>
            </ul>
            "#,
        )),
        "/health" => Ok(page_shell("health", "<p>ok</p>")),
        "/journal" => {
            let mut stmt = conn
                .prepare(
                    "SELECT title, entry_date, body_md, tags_json FROM journal_entries
                     ORDER BY entry_date DESC LIMIT 100",
                )
                .map_err(|e| AppError::Message(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                })
                .map_err(|e| AppError::Message(e.to_string()))?;
            let mut html = String::from("<h1>Journal</h1>");
            for r in rows {
                let (title, date, body, tags) = r.map_err(|e| AppError::Message(e.to_string()))?;
                html.push_str(&format!(
                    "<article><h2>{} <small>{}</small></h2><p class=\"meta\">{}</p><pre>{}</pre></article>",
                    esc(&title),
                    esc(&date),
                    esc(tags.as_deref().unwrap_or("")),
                    esc(&body)
                ));
            }
            Ok(page_shell("Journal", &html))
        }
        "/bibliography" => {
            let mut stmt = conn
                .prepare(
                    "SELECT title, authors, year, doi, url FROM bibliography_items
                     ORDER BY created_at DESC LIMIT 200",
                )
                .map_err(|e| AppError::Message(e.to_string()))?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                })
                .map_err(|e| AppError::Message(e.to_string()))?;
            let mut html = String::from("<h1>Bibliography</h1><ol>");
            for r in rows {
                let (title, authors, year, doi, url) =
                    r.map_err(|e| AppError::Message(e.to_string()))?;
                html.push_str(&format!(
                    "<li><strong>{}</strong><br/>{} ({}) {} {}</li>",
                    esc(&title),
                    esc(authors.as_deref().unwrap_or("")),
                    esc(year.as_deref().unwrap_or("n.d.")),
                    esc(doi.as_deref().unwrap_or("")),
                    url.as_ref()
                        .map(|u| format!(r#"<a href="{}">link</a>"#, esc(u)))
                        .unwrap_or_default()
                ));
            }
            html.push_str("</ol>");
            Ok(page_shell("Bibliography", &html))
        }
        _ => Ok(page_shell("404", "<p>Not found. <a href=\"/\">Home</a></p>")),
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn page_shell(title: &str, body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8"/><title>{title}</title>
<style>
 body{{font-family:system-ui,sans-serif;max-width:860px;margin:2rem auto;padding:0 1rem;line-height:1.5}}
 pre{{white-space:pre-wrap;background:#f5f5f5;padding:12px;border-radius:8px}}
 .meta{{color:#666;font-size:.9rem}}
 article{{border-bottom:1px solid #ddd;padding:1rem 0}}
 a{{color:#2f6fed}}
</style></head><body>
<nav><a href="/">Home</a> · <a href="/journal">Journal</a> · <a href="/bibliography">Bibliography</a></nav>
{body}
</body></html>"#
    )
}

