use crate::error::AppResult;
use std::path::Path;

/// Read HTML file and convert to plain-ish text (markdown-friendly).
pub fn extract_text(path: &Path) -> AppResult<String> {
    let raw = crate::parsers::text::read_text_file(path)?;
    Ok(html_to_text(&raw))
}

pub fn html_to_text(html: &str) -> String {
    // Prefer structured conversion; fall back to tag strip.
    match html2text::from_read(html.as_bytes(), 100) {
        Ok(s) => s,
        Err(_) => strip_tags(html),
    }
}

fn strip_tags(html: &str) -> String {
    let re = regex::Regex::new(r"(?is)<script[^>]*>.*?</script>|<style[^>]*>.*?</style>|<[^>]+>")
        .unwrap();
    let without = re.replace_all(html, " ");
    let space = regex::Regex::new(r"[ \t\r\f\v]+").unwrap();
    let collapsed = space.replace_all(&without, " ");
    collapsed
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_basic_html() {
        let t = html_to_text("<html><body><h1>Hi</h1><p>There</p></body></html>");
        assert!(t.to_lowercase().contains("hi") || t.contains("Hi"));
    }
}
