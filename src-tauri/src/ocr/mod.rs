//! Optional OCR via system Tesseract binary (if installed).

use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Serialize)]
pub struct OcrResult {
    pub text: String,
    pub engine: String,
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OcrStatus {
    pub available: bool,
    pub version: Option<String>,
    pub message: String,
}

pub fn tesseract_status() -> OcrStatus {
    match Command::new("tesseract").arg("--version").output() {
        Ok(out) if out.status.success() => {
            let v = String::from_utf8_lossy(&out.stdout);
            let first = v.lines().next().unwrap_or("tesseract").to_string();
            OcrStatus {
                available: true,
                version: Some(first),
                message: "Tesseract found. OCR available for image files.".into(),
            }
        }
        Ok(_) | Err(_) => OcrStatus {
            available: false,
            version: None,
            message: "Tesseract not found in PATH. Install tesseract-ocr to enable OCR.".into(),
        },
    }
}

/// Run OCR on an image path (png/jpg/tiff). PDF multi-page needs external pdftoppm — not included.
pub fn ocr_image(path: &str, lang: Option<String>) -> AppResult<OcrResult> {
    let p = Path::new(path);
    if !p.is_file() {
        return Err(AppError::Message(format!("file not found: {path}")));
    }
    let status = tesseract_status();
    if !status.available {
        return Err(AppError::Message(status.message));
    }
    let lang = lang.unwrap_or_else(|| "eng+rus".into());
    let output = Command::new("tesseract")
        .arg(path)
        .arg("stdout")
        .arg("-l")
        .arg(&lang)
        .output()
        .map_err(|e| AppError::Message(format!("tesseract spawn failed: {e}")))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Message(format!("tesseract failed: {err}")));
    }
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(OcrResult {
        text,
        engine: status.version.unwrap_or_else(|| "tesseract".into()),
        note: Some(format!("lang={lang}")),
    })
}
