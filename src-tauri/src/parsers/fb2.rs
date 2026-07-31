//! FictionBook 2 (.fb2) — XML text extract to markdown-ish body.

use crate::error::{AppError, AppResult};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

pub fn extract_text(path: &Path) -> AppResult<String> {
    let file = File::open(path)?;
    let mut reader = Reader::from_reader(BufReader::new(file));
    reader.config_mut().trim_text(true);

    let mut out = String::new();
    let mut buf = Vec::new();
    let mut in_body = false;
    let mut in_title = false;
    let mut in_p = false;
    let mut in_v = false; // verse line
    let mut skip_binary = false;
    let mut tag_stack: Vec<String> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                tag_stack.push(name.clone());
                match name.as_str() {
                    "body" => in_body = true,
                    "binary" => skip_binary = true,
                    "book-title" | "title" if !in_body => in_title = true,
                    "p" | "subtitle" if in_body => {
                        in_p = true;
                        if !out.ends_with('\n') && !out.is_empty() {
                            out.push('\n');
                        }
                    }
                    "v" if in_body => in_v = true,
                    "section" if in_body => {
                        out.push_str("\n\n");
                    }
                    "empty-line" if in_body => out.push('\n'),
                    "emphasis" | "strong" => {}
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                if name == "empty-line" && in_body {
                    out.push('\n');
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                match name.as_str() {
                    "body" => in_body = false,
                    "binary" => skip_binary = false,
                    "book-title" | "title" => {
                        if in_title {
                            out.push_str("\n\n");
                            in_title = false;
                        }
                    }
                    "p" | "subtitle" => {
                        if in_p {
                            out.push_str("\n\n");
                            in_p = false;
                        }
                    }
                    "v" => {
                        if in_v {
                            out.push('\n');
                            in_v = false;
                        }
                    }
                    "section" => out.push('\n'),
                    _ => {}
                }
                tag_stack.pop();
            }
            Ok(Event::Text(t)) => {
                if skip_binary {
                    buf.clear();
                    continue;
                }
                if let Ok(s) = t.unescape() {
                    let s = s.trim();
                    if s.is_empty() {
                        buf.clear();
                        continue;
                    }
                    if in_title && !in_body {
                        if out.is_empty() {
                            out.push_str("# ");
                        }
                        out.push_str(s);
                        out.push(' ');
                    } else if in_body && (in_p || in_v || tag_stack.iter().any(|t| t == "title")) {
                        out.push_str(s);
                        out.push(' ');
                    }
                }
            }
            Ok(Event::CData(t)) => {
                if in_body && !skip_binary {
                    if let Ok(s) = std::str::from_utf8(&t) {
                        out.push_str(s);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(AppError::Message(format!("fb2 xml error: {e}")));
            }
            _ => {}
        }
        buf.clear();
    }

    let cleaned = out
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");

    if cleaned.trim().is_empty() {
        return Err(AppError::Message(
            "FB2: no text found (empty or image-only book)".into(),
        ));
    }
    Ok(cleaned)
}
