//! DjVu text extract via DjVuLibre tools when present (optional dependency).

use crate::error::{AppError, AppResult};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Always returns Ok — never blocks opening the book.
/// If tools missing, returns a readable markdown stub with install help.
pub fn extract_text(path: &Path) -> AppResult<String> {
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");

    if let Some(bin) = find_djvutxt() {
        if let Some(t) = run_djvutxt(&bin, path) {
            if t.chars().filter(|c| !c.is_whitespace()).count() > 20 {
                return Ok(cleanup(&t));
            }
        }
    }

    // Soft fallback — open in reader with instructions (not a hard error)
    Ok(format!(
        "# {title}\n\n\
         **Формат DjVu**\n\n\
         Чтобы читать текст внутри SoheiDesk, нужен DjVuLibre (один раз):\n\n\
         ```bash\n\
         brew install djvulibre\n\
         ```\n\n\
         Затем снова откройте этот файл.\n\n\
         Альтернатива: сконвертируйте в PDF (например через [djvu2pdf](https://github.com) \
         или онлайн) и откройте PDF.\n\n\
         Путь: `{path}`\n",
        path = path.display()
    ))
}

fn find_djvutxt() -> Option<PathBuf> {
    let candidates = [
        "djvutxt",
        "/opt/homebrew/bin/djvutxt",
        "/usr/local/bin/djvutxt",
        "/usr/bin/djvutxt",
        "/opt/local/bin/djvutxt",
    ];
    for c in candidates {
        let p = PathBuf::from(c);
        if c == "djvutxt" {
            // which-like
            if Command::new(c).arg("-help").output().is_ok()
                || Command::new(c).arg("--help").output().is_ok()
            {
                // many versions exit non-zero on help but exist
            }
            if which_exists(c) {
                return Some(p);
            }
        } else if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// Report whether the optional DjVuLibre extractor can be started. The
/// diagnostics layer intentionally receives only a boolean, never the resolved
/// executable path.
pub fn tool_available() -> bool {
    find_djvutxt().is_some()
}

fn which_exists(bin: &str) -> bool {
    Command::new(bin)
        .arg("--help")
        .output()
        .map(|_| true)
        .unwrap_or(false)
}

fn run_djvutxt(bin: &Path, path: &Path) -> Option<String> {
    let out = Command::new(bin).arg(path).output().ok()?;
    if !out.status.success() && out.stdout.is_empty() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).into_owned();
    if s.trim().is_empty() {
        None
    } else {
        Some(s)
    }
}

fn cleanup(s: &str) -> String {
    s.lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

#[allow(dead_code)]
pub fn tools_missing_error() -> AppError {
    AppError::Message(
        "DJVU: install DjVuLibre once: brew install djvulibre — then reopen the file.".into(),
    )
}
