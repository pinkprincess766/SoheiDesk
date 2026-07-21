use crate::error::AppResult;
use encoding_rs::{UTF_8, WINDOWS_1251};
use std::fs;
use std::path::Path;

/// Read a text file as UTF-8, falling back to Windows-1251 then lossy UTF-8.
pub fn read_text_file(path: &Path) -> AppResult<String> {
    let bytes = fs::read(path)?;

    if let Ok(s) = std::str::from_utf8(&bytes) {
        return Ok(s.to_string());
    }

    let (cow, _, had_errors) = UTF_8.decode(&bytes);
    if !had_errors {
        return Ok(cow.into_owned());
    }

    let (cow1251, _, had_errors_1251) = WINDOWS_1251.decode(&bytes);
    if !had_errors_1251 {
        return Ok(cow1251.into_owned());
    }

    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
