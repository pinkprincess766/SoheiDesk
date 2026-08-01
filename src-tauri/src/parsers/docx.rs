use crate::error::{AppError, AppResult};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

/// Extract Markdown from .docx.
/// Images written under `media_dir` as files and linked via `sohei-file://{abs}` when possible.
pub fn extract_text(path: &Path, media_dir: Option<&Path>) -> AppResult<String> {
    let file = std::fs::File::open(path)?;
    let mut archive =
        ZipArchive::new(file).map_err(|e| AppError::Message(format!("invalid docx/zip: {e}")))?;

    let rels = read_document_rels(&mut archive)?;
    if let Some(dir) = media_dir {
        std::fs::create_dir_all(dir)?;
    }

    let mut image_md: HashMap<String, String> = HashMap::new();
    let mut img_index = 0usize;
    for (rid, target) in &rels {
        if let Some(md) = materialize_image(&mut archive, target, media_dir, &mut img_index) {
            image_md.insert(rid.clone(), md);
        }
    }

    let media_names: Vec<String> = {
        let mut names = Vec::new();
        for i in 0..archive.len() {
            if let Ok(f) = archive.by_index(i) {
                let name = f.name().replace('\\', "/");
                if name.starts_with("word/media/") && !name.ends_with('/') {
                    names.push(name);
                }
            }
        }
        names
    };

    let mut doc_xml = String::new();
    {
        let mut doc = archive
            .by_name("word/document.xml")
            .map_err(|e| AppError::Message(format!("docx missing word/document.xml: {e}")))?;
        doc.read_to_string(&mut doc_xml)?;
    }

    let mut text = xml_to_markdown(&doc_xml, &image_md);

    if !text.contains("sohei-file://") && !text.contains("data:image") && !media_names.is_empty() {
        text.push_str("\n\n## Images\n\n");
        for name in media_names {
            if let Some(md) = materialize_image(&mut archive, &name, media_dir, &mut img_index) {
                text.push_str(&md);
                text.push_str("\n\n");
            }
        }
    }

    Ok(text)
}

fn materialize_image(
    archive: &mut ZipArchive<std::fs::File>,
    zip_path: &str,
    media_dir: Option<&Path>,
    img_index: &mut usize,
) -> Option<String> {
    let path_in_zip = normalize_zip_path(zip_path);
    let mut file = archive.by_name(&path_in_zip).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    if bytes.is_empty() {
        return None;
    }

    let ext = Path::new(&path_in_zip)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin")
        .to_ascii_lowercase();

    if let Some(dir) = media_dir {
        let fname = format!("img_{img_index}.{ext}");
        *img_index += 1;
        let dest: PathBuf = dir.join(&fname);
        if std::fs::write(&dest, &bytes).is_ok() {
            let abs = dest.canonicalize().unwrap_or(dest);
            return Some(format!("![image](sohei-file://{})", abs.to_string_lossy()));
        }
    }

    if bytes.len() <= 3 * 1024 * 1024 {
        let mime = mime_from_ext(&ext);
        let b64 = B64.encode(&bytes);
        *img_index += 1;
        return Some(format!("![image](data:{mime};base64,{b64})"));
    }

    *img_index += 1;
    Some(format!(
        "\n> *[image omitted: large or unsupported ({ext})]*\n"
    ))
}

fn normalize_zip_path(target: &str) -> String {
    let t = target.replace('\\', "/");
    if t.starts_with("word/") {
        t
    } else if t.starts_with("/word/") {
        t.trim_start_matches('/').to_string()
    } else if let Some(rest) = t.strip_prefix("../") {
        format!("word/{rest}")
    } else {
        format!("word/{t}")
    }
}

fn mime_from_ext(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "tif" | "tiff" => "image/tiff",
        _ => "application/octet-stream",
    }
}

fn read_document_rels(
    archive: &mut ZipArchive<std::fs::File>,
) -> AppResult<HashMap<String, String>> {
    let mut map = HashMap::new();
    let mut rels_xml = String::new();
    match archive.by_name("word/_rels/document.xml.rels") {
        Ok(mut f) => {
            f.read_to_string(&mut rels_xml)?;
        }
        Err(_) => return Ok(map),
    }

    let re = regex::Regex::new(r#"(?is)<Relationship\b([^>]+)>"#).unwrap();
    let re_id = regex::Regex::new(r#"(?i)\bId\s*=\s*"([^"]+)""#).unwrap();
    let re_target = regex::Regex::new(r#"(?i)\bTarget\s*=\s*"([^"]+)""#).unwrap();

    for cap in re.captures_iter(&rels_xml) {
        let attrs = &cap[1];
        let id = re_id.captures(attrs).map(|c| c[1].to_string());
        let target = re_target.captures(attrs).map(|c| c[1].to_string());
        if let (Some(id), Some(target)) = (id, target) {
            map.insert(id, target.replace('\\', "/"));
        }
    }
    Ok(map)
}

fn xml_to_markdown(xml: &str, images: &HashMap<String, String>) -> String {
    let mut reader = Reader::from_reader(Cursor::new(xml.as_bytes()));
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut in_t = false;
    let mut pending_embeds: Vec<String> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name().local_name();
                let n = name.as_ref();
                if n == b"t" {
                    in_t = true;
                } else if n == b"tab" {
                    out.push('\t');
                } else if n == b"br" || n == b"cr" {
                    out.push('\n');
                } else if n == b"blip" || n == b"imagedata" {
                    collect_embed_attrs(e, &mut pending_embeds);
                }
            }
            Ok(Event::Empty(ref e)) => {
                let name = e.name().local_name();
                let n = name.as_ref();
                if n == b"tab" {
                    out.push('\t');
                } else if n == b"br" || n == b"cr" {
                    out.push('\n');
                } else if n == b"blip" || n == b"imagedata" {
                    collect_embed_attrs(e, &mut pending_embeds);
                    flush_embeds(&mut out, &mut pending_embeds, images);
                } else if n == b"drawing" || n == b"pict" {
                    flush_embeds(&mut out, &mut pending_embeds, images);
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name().local_name();
                let n = name.as_ref();
                if n == b"t" {
                    in_t = false;
                } else if n == b"p" {
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push('\n');
                } else if n == b"drawing" || n == b"pict" || n == b"object" {
                    flush_embeds(&mut out, &mut pending_embeds, images);
                }
            }
            Ok(Event::Text(ref t)) => {
                if in_t {
                    if let Ok(decoded) = t.decode() {
                        out.push_str(&decoded);
                    }
                }
            }
            Ok(Event::GeneralRef(reference)) => {
                if in_t {
                    push_xml_reference(&mut out, &reference);
                }
            }
            Ok(Event::CData(ref text)) => {
                if in_t {
                    if let Ok(decoded) = text.decode() {
                        out.push_str(&decoded);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    flush_embeds(&mut out, &mut pending_embeds, images);

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

fn push_xml_reference(out: &mut String, reference: &quick_xml::events::BytesRef<'_>) {
    if let Ok(Some(character)) = reference.resolve_char_ref() {
        out.push(character);
    } else if let Ok(name) = reference.decode() {
        if let Some(value) = quick_xml::escape::resolve_xml_entity(&name) {
            out.push_str(value);
        } else {
            // Preserve unknown named entities without expanding untrusted DTD content.
            out.push('&');
            out.push_str(&name);
            out.push(';');
        }
    }
}

fn collect_embed_attrs(e: &quick_xml::events::BytesStart<'_>, pending: &mut Vec<String>) {
    for attr in e.attributes().flatten() {
        let key = attr.key.local_name();
        let k = key.as_ref();
        if k == b"embed" || k == b"id" || k == b"link" {
            if let Ok(v) =
                attr.decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, e.decoder())
            {
                let s = v.to_string();
                if !s.is_empty() {
                    pending.push(s);
                }
            }
        }
    }
}

fn flush_embeds(out: &mut String, pending: &mut Vec<String>, images: &HashMap<String, String>) {
    if pending.is_empty() {
        return;
    }
    let mut seen = std::collections::HashSet::new();
    for rid in pending.drain(..) {
        if !seen.insert(rid.clone()) {
            continue;
        }
        if let Some(md) = images.get(&rid) {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(md);
            out.push_str("\n\n");
        } else {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&format!("\n> *[image: {rid}]*\n\n"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_extracts_text_runs() {
        let xml = r#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
          <w:body><w:p><w:r><w:t>Hello</w:t></w:r><w:r><w:t xml:space="preserve"> world &amp; XML</w:t></w:r></w:p></w:body>
        </w:document>"#;
        let t = xml_to_markdown(xml, &HashMap::new());
        assert!(t.contains("Hello"));
        assert!(t.contains("world"));
        assert!(t.contains("world & XML"));
    }

    #[test]
    fn inserts_image_markdown() {
        let xml = r#"<?xml version="1.0"?>
        <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
          xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
          xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
          <w:body>
            <w:p><w:r><w:t>Before</w:t></w:r></w:p>
            <w:p><w:r><w:drawing><a:blip r:embed="rId5"/></w:drawing></w:r></w:p>
            <w:p><w:r><w:t>After</w:t></w:r></w:p>
          </w:body>
        </w:document>"#;
        let mut imgs = HashMap::new();
        imgs.insert("rId5".into(), "![image](data:image/png;base64,aaa)".into());
        let t = xml_to_markdown(xml, &imgs);
        assert!(t.contains("Before"));
        assert!(t.contains("data:image/png;base64,aaa"));
        assert!(t.contains("After"));
    }

    #[test]
    fn extracts_real_fixture_with_image() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../fixtures/sample-with-image.docx");
        if !path.is_file() {
            return;
        }
        let dir = std::env::temp_dir().join("sohei_docx_test_media2");
        let _ = std::fs::create_dir_all(&dir);
        let text = extract_text(&path, Some(&dir)).expect("extract fixture");
        assert!(text.contains("Hello with image"));
        assert!(
            text.contains("sohei-file://") || text.contains("data:image"),
            "expected image ref, got: {}",
            text.chars().take(300).collect::<String>()
        );
    }
}
