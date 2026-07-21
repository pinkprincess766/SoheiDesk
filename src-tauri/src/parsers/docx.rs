use crate::error::{AppError, AppResult};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::{Cursor, Read};
use std::path::Path;
use zip::ZipArchive;

/// Extract plain text from a .docx (Office Open XML) file.
pub fn extract_text(path: &Path) -> AppResult<String> {
    let file = std::fs::File::open(path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| AppError::Message(format!("invalid docx/zip: {e}")))?;

    let mut doc = archive
        .by_name("word/document.xml")
        .map_err(|e| AppError::Message(format!("docx missing word/document.xml: {e}")))?;

    let mut xml = String::new();
    doc.read_to_string(&mut xml)?;
    Ok(xml_to_text(&xml))
}

fn xml_to_text(xml: &str) -> String {
    let mut reader = Reader::from_reader(Cursor::new(xml.as_bytes()));
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut in_t = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let local = name.local_name();
                if local.as_ref() == b"t" {
                    in_t = true;
                } else if local.as_ref() == b"tab" {
                    out.push('\t');
                } else if local.as_ref() == b"br" || local.as_ref() == b"cr" {
                    out.push('\n');
                }
            }
            Ok(Event::Empty(e)) => {
                let name = e.name();
                let local = name.local_name();
                if local.as_ref() == b"tab" {
                    out.push('\t');
                } else if local.as_ref() == b"br" || local.as_ref() == b"cr" {
                    out.push('\n');
                } else if local.as_ref() == b"p" {
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let local = name.local_name();
                if local.as_ref() == b"t" {
                    in_t = false;
                } else if local.as_ref() == b"p" {
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if in_t {
                    if let Ok(s) = t.unescape() {
                        out.push_str(&s);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    // collapse excessive blank lines
    let mut cleaned = String::new();
    let mut blank = 0;
    for line in out.lines() {
        if line.trim().is_empty() {
            blank += 1;
            if blank <= 2 {
                cleaned.push('\n');
            }
        } else {
            blank = 0;
            cleaned.push_str(line);
            cleaned.push('\n');
        }
    }
    cleaned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_extracts_text_runs() {
        let xml = r#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body><w:p><w:r><w:t>Hello</w:t></w:r><w:r><w:t xml:space="preserve"> world</w:t></w:r></w:p></w:body>
        </w:document>"#;
        let t = xml_to_text(xml);
        assert!(t.contains("Hello"));
        assert!(t.contains("world"));
    }
}
