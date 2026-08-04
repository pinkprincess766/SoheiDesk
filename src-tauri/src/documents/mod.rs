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
    Fb2,
    Djvu,
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
            DocType::Fb2 => "fb2",
            DocType::Djvu => "djvu",
        }
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
            "fb2" => Ok(DocType::Fb2),
            "djvu" | "djv" => Ok(DocType::Djvu),
            other => Err(AppError::Message(format!(
                "unsupported file type: .{other}"
            ))),
        }
    }
}

/// Full-file SHA-256 used as the durable document identity.
pub fn content_hash(path: &Path) -> AppResult<(String, u64)> {
    let mut file = File::open(path)?;
    let file_size = file.metadata()?.len();

    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok((hex::encode(hasher.finalize()), file_size))
}

/// Compatibility fingerprint used only to recognize rows created before v5.
/// It must not be exposed as SHA-256 because bytes in the middle were skipped.
pub(crate) fn legacy_content_hash(path: &Path) -> AppResult<(String, u64)> {
    let mut file = File::open(path)?;
    let file_size = file.metadata()?.len();
    let mut hasher = Sha256::new();
    hasher.update(file_size.to_le_bytes());

    if file_size == 0 {
        return Ok((hex::encode(hasher.finalize()), 0));
    }
    if file_size <= HASH_CHUNK * 2 {
        let mut buffer = Vec::with_capacity(file_size as usize);
        file.read_to_end(&mut buffer)?;
        hasher.update(buffer);
    } else {
        let mut head = vec![0_u8; HASH_CHUNK as usize];
        file.read_exact(&mut head)?;
        hasher.update(head);
        file.seek(SeekFrom::End(-(HASH_CHUNK as i64)))?;
        let mut tail = vec![0_u8; HASH_CHUNK as usize];
        file.read_exact(&mut tail)?;
        hasher.update(tail);
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
    fn content_hash_is_standard_sha256() {
        let path = std::env::temp_dir().join(format!(
            "sohei_hash_vector_{}.txt",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::write(&path, b"abc").unwrap();
        let (hash, size) = content_hash(&path).unwrap();
        assert_eq!(
            hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(size, 3);
        let _ = std::fs::remove_file(path);
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
    fn full_hash_detects_changes_hidden_from_legacy_fingerprint() {
        let path = std::env::temp_dir().join(format!(
            "sohei_hash_middle_{}.bin",
            uuid::Uuid::new_v4().simple()
        ));
        let mut payload = vec![b'a'; (HASH_CHUNK * 3) as usize];
        std::fs::write(&path, &payload).unwrap();
        let (full_before, _) = content_hash(&path).unwrap();
        let (legacy_before, _) = legacy_content_hash(&path).unwrap();

        payload[HASH_CHUNK as usize + 10] = b'b';
        std::fs::write(&path, &payload).unwrap();
        let (full_after, _) = content_hash(&path).unwrap();
        let (legacy_after, _) = legacy_content_hash(&path).unwrap();

        assert_ne!(full_before, full_after);
        assert_eq!(legacy_before, legacy_after);
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
