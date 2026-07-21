use crate::error::{AppError, AppResult};
use epub::doc::EpubDoc;
use std::path::Path;

/// Extract concatenated chapter text from an EPUB (best-effort, no DRM).
pub fn extract_text(path: &Path) -> AppResult<String> {
    let mut doc = EpubDoc::new(path)
        .map_err(|e| AppError::Message(format!("epub open failed: {e}")))?;

    let mut parts: Vec<String> = Vec::new();

    // title via Debug (value field is crate-private in epub 2.x)
    if let Some(title) = doc.mdata("title") {
        let dbg = format!("{title:?}");
        if let Some(v) = extract_debug_field(&dbg, "value") {
            parts.push(format!("# {v}\n\n"));
        }
    }

    let spine_len = doc.get_num_chapters();
    for i in 0..spine_len {
        if !doc.set_current_chapter(i) {
            continue;
        }
        if let Some((content, _mime)) = doc.get_current_str() {
            let plain = crate::parsers::html::html_to_text(&content);
            if !plain.trim().is_empty() {
                parts.push(plain);
                parts.push("\n\n---\n\n".into());
            }
        }
    }

    if parts.is_empty() {
        return Err(AppError::Message("epub has no readable content".into()));
    }
    Ok(parts.join(""))
}

fn extract_debug_field(dbg: &str, field: &str) -> Option<String> {
    let key = format!("{field}: \"");
    let start = dbg.find(&key)? + key.len();
    let rest = &dbg[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}
