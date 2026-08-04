pub mod djvu;
pub mod docx;
pub mod epub_parse;
pub mod fb2;
pub mod html;
pub mod pdf_text;
pub mod text;

use crate::documents::DocType;
use crate::error::{AppError, AppResult};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct OpenedDocument {
    pub path: String,
    pub doc_type: String,
    pub title: String,
    pub content_hash: String,
    pub file_size: u64,
    /// Text/reflow body; empty for pdf.
    pub text: Option<String>,
    /// PDF bytes as base64 (small/medium files) so the UI does not re-read by path.
    pub binary_base64: Option<String>,
    /// App-local cache copy path (PDF / extracted media root).
    pub cache_path: Option<String>,
}

/// Open a document after its identity was already computed by the library.
/// Reusing the verified digest avoids a second full read of large documents.
pub fn open_document_with_identity(
    path: &Path,
    cache_dir: Option<&Path>,
    content_hash: String,
    file_size: u64,
) -> AppResult<OpenedDocument> {
    if !path.is_file() {
        return Err(AppError::Message(format!(
            "file not found: {}",
            path.display()
        )));
    }

    let doc_type = DocType::from_path(path)?;
    let title = crate::documents::title_from_path(path);

    // Ensure cache dir when provided
    if let Some(dir) = cache_dir {
        std::fs::create_dir_all(dir)?;
    }

    let mut binary_base64 = None;
    let mut cache_path = None;

    let text = match doc_type {
        DocType::Txt | DocType::Md | DocType::Tex => Some(text::read_text_file(path)?),
        DocType::Docx => Some(docx::extract_text(path, cache_dir)?),
        DocType::Epub => Some(epub_parse::extract_text(path)?),
        DocType::Html => Some(html::extract_text(path)?),
        DocType::Fb2 => Some(fb2::extract_text(path)?),
        DocType::Djvu => Some(djvu::extract_text(path)?),
        DocType::Pdf => {
            // Always make a cache copy under app data (asset protocol friendly)
            if let Some(dir) = cache_dir {
                let dest = dir.join("document.pdf");
                if !dest.is_file() || dest.metadata().map(|m| m.len()).unwrap_or(0) != file_size {
                    std::fs::copy(path, &dest)
                        .map_err(|e| AppError::Message(format!("failed to cache PDF: {e}")))?;
                }
                cache_path = Some(dest.to_string_lossy().to_string());
            }
            // Prefer cache_path + authorized read / asset protocol.
            // Avoid large base64 in the Vue/Pinia reactive graph (freezes UI on reopen).
            if cache_path.is_none() && file_size > 0 && file_size <= 2 * 1024 * 1024 {
                let bytes = std::fs::read(path)?;
                binary_base64 = Some(B64.encode(bytes));
            }
            // Text layer for Simple mode + search (cached as body.txt)
            match pdf_text::extract_and_cache(path, cache_dir) {
                Ok(t) => Some(t),
                Err(e) => {
                    // Don't fail open — Normal mode can still show pages
                    Some(format!(
                        "# {title}\n\n(Не удалось извлечь текст PDF: {e})\n\n\
                         В Обычном режиме откройте страницы документа.\n"
                    ))
                }
            }
        }
    };

    Ok(OpenedDocument {
        path: path.to_string_lossy().to_string(),
        doc_type: doc_type.as_str().to_string(),
        title,
        content_hash,
        file_size,
        text,
        binary_base64,
        cache_path,
    })
}

/// Extract searchable text for indexing.
pub fn extract_search_text(path: &Path, doc_type: &str) -> AppResult<String> {
    match doc_type {
        "pdf" => pdf_text::extract_text(path).or_else(|_| Ok(String::new())),
        "txt" | "md" | "rtf" | "tex" => text::read_text_file(path),
        "docx" => {
            // index without writing media
            docx::extract_text(path, None).map(|t| {
                // strip data URIs / sohei-file markers for index size
                let re = regex::Regex::new(r"!\[[^\]]*\]\((data:|sohei-file://)[^)]+\)").ok();
                if let Some(re) = re {
                    re.replace_all(&t, " ").to_string()
                } else {
                    t
                }
            })
        }
        "epub" => epub_parse::extract_text(path),
        "html" => html::extract_text(path),
        "fb2" => fb2::extract_text(path),
        "djvu" => djvu::extract_text(path).or_else(|_| Ok(String::new())),
        _ => {
            if path.is_file() {
                text::read_text_file(path).or_else(|_| Ok(String::new()))
            } else {
                Ok(String::new())
            }
        }
    }
}

#[allow(dead_code)]
pub fn cache_root(data_dir: &Path, content_hash: &str) -> PathBuf {
    data_dir.join("media").join(content_hash)
}
