//! Paths passed at launch (double-click / Open With / CLI args).

use parking_lot::Mutex;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct PendingOpen {
    paths: Mutex<Vec<String>>,
}

impl PendingOpen {
    pub fn push_many(&self, paths: impl IntoIterator<Item = String>) {
        let mut g = self.paths.lock();
        for p in paths {
            if !p.is_empty() && !g.contains(&p) {
                g.push(p);
            }
        }
    }

    pub fn take_all(&self) -> Vec<String> {
        std::mem::take(&mut *self.paths.lock())
    }
}

pub fn is_supported_path(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "pdf"
            | "md"
            | "markdown"
            | "txt"
            | "text"
            | "log"
            | "docx"
            | "epub"
            | "html"
            | "htm"
            | "tex"
            | "latex"
            | "ltx"
            | "fb2"
            | "djvu"
            | "djv"
            | "rtf"
    )
}

/// Collect file paths from process args (Windows/Linux “Open with”).
pub fn paths_from_env_args() -> Vec<String> {
    std::env::args()
        .skip(1)
        .filter_map(|a| {
            if a.starts_with('-') {
                return None;
            }
            let p = PathBuf::from(&a);
            if p.is_file() && is_supported_path(&p) {
                Some(p.to_string_lossy().to_string())
            } else {
                None
            }
        })
        .collect()
}
