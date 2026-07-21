use crate::error::AppResult;
use crate::ocr::{self, OcrResult, OcrStatus};

#[tauri::command]
pub fn ocr_status() -> OcrStatus {
    ocr::tesseract_status()
}

#[tauri::command]
pub fn ocr_image(path: String, lang: Option<String>) -> AppResult<OcrResult> {
    ocr::ocr_image(&path, lang)
}
