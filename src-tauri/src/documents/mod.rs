use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Bytes from start and end used for content fingerprint of large files.
const HASH_CHUNK: u64 = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocType {
    Pdf,
    Md,
    Txt,
    Docx,
    Epub,
    Html,
    Tex,
}

impl DocType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DocType::Pdf => "pdf",
            DocType::Md => "md",
            DocType::Txt => "txt",
            DocType::Docx => "docx",
            DocType::Epub => "epub",
            DocType::Html => "html",
            DocType::Tex => "tex",
        }
    }

    pub fn is_reflow(&self) -> bool {
        matches!(
            self,
            DocType::Md
                | DocType::Txt
                | DocType::Docx
                | DocType::Epub
                | DocType::Html
                | DocType::Tex
        )
    }

    pub fn from_path(path: &Path) -> AppResult<Self> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "pdf" => Ok(DocType::Pdf),
            "md" | "markdown" => Ok(DocType::Md),
            "txt" | "text" | "log" | "rtf" => Ok(DocType::Txt),
            "docx" => Ok(DocType::Docx),
            "epub" => Ok(DocType::Epub),
            "html" | "htm" => Ok(DocType::Html),
            "tex" | "latex" | "ltx" => Ok(DocType::Tex),
            other => Err(AppError::Message(format!(
                "unsupported file type: .{other}"
            ))),
        }
    }
}

/// Stable content fingerprint: size || head || tail (SHA-256 hex).
pub fn content_hash(path: &Path) -> AppResult<(String, u64)> {
    let mut file = File::open(path)?;
    let file_size = file.metadata()?.len();

    let mut hasher = Sha256::new();
    hasher.update(file_size.to_le_bytes());

    if file_size == 0 {
        return Ok((hex::encode(hasher.finalize()), 0));
    }

    if file_size <= HASH_CHUNK * 2 {
        let mut buf = Vec::with_capacity(file_size as usize);
        file.read_to_end(&mut buf)?;
        hasher.update(&buf);
    } else {
        let mut head = vec![0u8; HASH_CHUNK as usize];
        file.read_exact(&mut head)?;
        hasher.update(&head);

        file.seek(SeekFrom::End(-(HASH_CHUNK as i64)))?;
        let mut tail = vec![0u8; HASH_CHUNK as usize];
        file.read_exact(&mut tail)?;
        hasher.update(&tail);
    }

    Ok((hex::encode(hasher.finalize()), file_size))
}

pub fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn content_hash_stable_for_same_bytes() {
        let dir = std::env::temp_dir();
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p1 = dir.join(format!("sohei_hash_a_{t}.txt"));
        let p2 = dir.join(format!("sohei_hash_b_{t}.txt"));
        let payload = b"hello soheidesk content hash";
        std::fs::write(&p1, payload).unwrap();
        std::fs::write(&p2, payload).unwrap();

        let (h1, s1) = content_hash(&p1).unwrap();
        let (h2, s2) = content_hash(&p2).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(s1, s2);
        assert_eq!(s1, payload.len() as u64);

        let _ = std::fs::remove_file(p1);
        let _ = std::fs::remove_file(p2);
    }

    #[test]
    fn content_hash_differs_when_content_changes() {
        let dir = std::env::temp_dir();
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = dir.join(format!("sohei_hash_c_{t}.txt"));
        {
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(b"version-one").unwrap();
        }
        let (h1, _) = content_hash(&path).unwrap();
        std::fs::write(&path, b"version-two").unwrap();
        let (h2, _) = content_hash(&path).unwrap();
        assert_ne!(h1, h2);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn doc_types_from_extensions() {
        assert_eq!(
            DocType::from_path(Path::new("a.docx")).unwrap(),
            DocType::Docx
        );
        assert_eq!(
            DocType::from_path(Path::new("a.epub")).unwrap(),
            DocType::Epub
        );
        assert_eq!(
            DocType::from_path(Path::new("a.HTML")).unwrap(),
            DocType::Html
        );
    }
}
