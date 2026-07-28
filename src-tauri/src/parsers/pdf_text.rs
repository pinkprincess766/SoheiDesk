//! PDF → plain text for Simple mode / search.
//! Prefer pdftotext; fallback pure-Rust pdf-extract + CP1251 mojibake repair.
//! Results cached as media/{hash}/body.txt

use crate::error::{AppError, AppResult};
use encoding_rs::WINDOWS_1251;
use std::path::Path;
use std::process::Command;

/// Extract text and optionally write `cache_dir/body.txt` for reuse.
pub fn extract_and_cache(path: &Path, cache_dir: Option<&Path>) -> AppResult<String> {
    if let Some(dir) = cache_dir {
        let body = dir.join("body.txt");
        if body.is_file() {
            if let Ok(s) = std::fs::read_to_string(&body) {
                if s.chars().filter(|c| !c.is_whitespace()).count() > 40 {
                    return Ok(s);
                }
            }
        }
    }

    let text = extract_text(path)?;

    if let Some(dir) = cache_dir {
        let _ = std::fs::create_dir_all(dir);
        let body = dir.join("body.txt");
        let _ = std::fs::write(&body, &text);
    }

    Ok(text)
}

pub fn extract_text(path: &Path) -> AppResult<String> {
    if let Some(t) = try_pdftotext(path) {
        if meaningful(&t) {
            return Ok(postprocess(&t));
        }
    }

    match pdf_extract::extract_text(path) {
        Ok(t) => {
            let cleaned = postprocess(&t);
            if meaningful(&cleaned) {
                Ok(cleaned)
            } else {
                Ok(empty_hint(path))
            }
        }
        Err(e) => Err(AppError::Message(format!("PDF text extract failed: {e}"))),
    }
}

fn meaningful(s: &str) -> bool {
    s.chars().filter(|c| !c.is_whitespace()).count() > 40
}

fn empty_hint(path: &Path) -> String {
    format!(
        "# {}\n\n\
         PDF: текстовый слой пуст или это скан (нужен OCR).\n\n\
         В **Обычном** режиме можно листать страницы как изображения.\n\n\
         path: {}\n",
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("PDF"),
        path.display()
    )
}

fn try_pdftotext(path: &Path) -> Option<String> {
    let bins = [
        "pdftotext",
        "/opt/homebrew/bin/pdftotext",
        "/usr/local/bin/pdftotext",
        "/usr/bin/pdftotext",
    ];
    for bin in bins {
        let out = Command::new(bin)
            .args(["-layout", "-enc", "UTF-8"])
            .arg(path)
            .arg("-")
            .output()
            .ok()?;
        if out.status.success() || !out.stdout.is_empty() {
            let s = String::from_utf8_lossy(&out.stdout).into_owned();
            if !s.trim().is_empty() {
                return Some(s);
            }
        }
    }
    None
}

fn postprocess(t: &str) -> String {
    cleanup_pdf_text(&fix_cyrillic_mojibake(t))
}

/// CP1251 bytes mis-decoded as Latin-1 → «ÃËÀÂÀ» instead of «ГЛАВА».
fn fix_cyrillic_mojibake(s: &str) -> String {
    s.lines()
        .map(fix_line_encoding)
        .collect::<Vec<_>>()
        .join("\n")
}

fn fix_line_encoding(line: &str) -> String {
    if line.is_empty() {
        return String::new();
    }
    let real_cyr = count_cyrillic(line);
    let moji = count_mojibake_latin(line);
    if real_cyr > 0 && real_cyr >= moji {
        return line.to_string();
    }
    if moji < 3 {
        return line.to_string();
    }
    let mut bytes = Vec::with_capacity(line.len());
    for c in line.chars() {
        let u = c as u32;
        if u <= 0xFF {
            bytes.push(u as u8);
        } else {
            return line.to_string();
        }
    }
    let (cow, _, had_errors) = WINDOWS_1251.decode(&bytes);
    if had_errors {
        return line.to_string();
    }
    let fixed = cow.into_owned();
    if count_cyrillic(&fixed) > real_cyr {
        fixed
    } else {
        line.to_string()
    }
}

fn is_cyrillic(c: char) -> bool {
    matches!(c, '\u{0400}'..='\u{04FF}' | '\u{0500}'..='\u{052F}')
}

fn count_cyrillic(s: &str) -> usize {
    s.chars().filter(|c| is_cyrillic(*c)).count()
}

fn count_mojibake_latin(s: &str) -> usize {
    s.chars()
        .filter(|c| {
            let u = *c as u32;
            !is_cyrillic(*c) && (0xC0..=0xFF).contains(&u)
        })
        .count()
}

fn cleanup_pdf_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank = 0u32;
    for line in s.lines() {
        let t = line
            .replace('\u{00A0}', " ")
            .replace('\u{000C}', "")
            .replace('\u{00AD}', "")
            .trim_end()
            .to_string();
        if t.is_empty() {
            blank += 1;
            if blank <= 2 {
                out.push('\n');
            }
        } else {
            blank = 0;
            out.push_str(&t);
            out.push('\n');
        }
    }
    out
}
