pub mod docx;
pub mod epub_parse;
pub mod html;
pub mod text;

use crate::documents::DocType;
use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct OpenedDocument {
    pub path: String,
    pub doc_type: String,
    pub title: String,
    pub content_hash: String,
    pub file_size: u64,
    /// Text/reflow body; empty for pdf.
    pub text: Option<String>,
}

pub fn open_document(path: &Path) -> AppResult<OpenedDocument> {
    if !path.is_file() {
        return Err(AppError::Message(format!(
            "file not found: {}",
            path.display()
        )));
    }

    let doc_type = DocType::from_path(path)?;
    let (content_hash, file_size) = crate::documents::content_hash(path)?;
    let title = crate::documents::title_from_path(path);

    let text = match doc_type {
        DocType::Txt | DocType::Md | DocType::Tex => Some(text::read_text_file(path)?),
        DocType::Docx => Some(docx::extract_text(path)?),
        DocType::Epub => Some(epub_parse::extract_text(path)?),
        DocType::Html => Some(html::extract_text(path)?),
        DocType::Pdf => None,
    };

    Ok(OpenedDocument {
        path: path.to_string_lossy().to_string(),
        doc_type: doc_type.as_str().to_string(),
        title,
        content_hash,
        file_size,
        text,
    })
}

/// Extract searchable text for indexing (including best-effort PDF skip).
pub fn extract_search_text(path: &Path, doc_type: &str) -> AppResult<String> {
    match doc_type {
        "pdf" => Ok(String::new()), // PDF text extract later; index title/path only
        "txt" | "md" | "rtf" | "tex" => text::read_text_file(path),
        "docx" => docx::extract_text(path),
        "epub" => epub_parse::extract_text(path),
        "html" => html::extract_text(path),
        _ => {
            if path.is_file() {
                // try as text
                text::read_text_file(path).or_else(|_| Ok(String::new()))
            } else {
                Ok(String::new())
            }
        }
    }
}
