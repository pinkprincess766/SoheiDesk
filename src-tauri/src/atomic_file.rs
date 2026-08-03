use crate::error::{AppError, AppResult};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Writes bytes to a verified sibling file and replaces the destination only
/// after the complete payload has reached the filesystem.
pub(crate) fn write_bytes(path: impl AsRef<Path>, bytes: &[u8]) -> AppResult<()> {
    write_file(
        path,
        |mut file| {
            file.write_all(bytes)?;
            Ok(file)
        },
        |temporary| {
            let stored = fs::read(temporary)?;
            if stored != bytes {
                return Err(AppError::Message(
                    "temporary export differs from the generated content".into(),
                ));
            }
            Ok(())
        },
    )
}

/// Gives structured writers, such as ZIP/DOCX exporters, the same atomic
/// replacement contract as byte-oriented exports.
pub(crate) fn write_file<W, V>(
    destination: impl AsRef<Path>,
    writer: W,
    validator: V,
) -> AppResult<()>
where
    W: FnOnce(File) -> AppResult<File>,
    V: FnOnce(&Path) -> AppResult<()>,
{
    let destination = destination.as_ref();
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if destination.file_name().is_none() {
        return Err(AppError::Message(
            "export destination must include a file name".into(),
        ));
    }

    let existing_permissions = match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(AppError::Message(
                "refusing to replace an export destination symlink".into(),
            ));
        }
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(AppError::Message(
                "export destination is not a regular file".into(),
            ));
        }
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };

    let (temporary, file) = create_temporary_file(parent)?;
    let mut cleanup = TemporaryCleanup::new(temporary.clone());
    let file = writer(file)?;
    if let Some(permissions) = existing_permissions {
        file.set_permissions(permissions)?;
    }
    file.sync_all()?;
    drop(file);

    validator(&temporary)?;
    sync_directory(parent)?;
    replace_destination(&temporary, destination)?;
    cleanup.disarm();
    sync_directory(parent)?;
    Ok(())
}

fn create_temporary_file(parent: &Path) -> AppResult<(PathBuf, File)> {
    for _ in 0..16 {
        let temporary_name = format!(".soheidesk-export-{}.tmp", Uuid::new_v4());
        let temporary = parent.join(temporary_name);
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(AppError::Message(
        "could not allocate a unique temporary export file".into(),
    ))
}

#[cfg(windows)]
fn replace_destination(temporary: &Path, destination: &Path) -> AppResult<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let temporary_wide =
        nul_terminated_windows_path(temporary.as_os_str().encode_wide().collect())?;
    let destination_wide =
        nul_terminated_windows_path(destination.as_os_str().encode_wide().collect())?;
    // Both paths are siblings, so MoveFileExW performs a single-volume atomic
    // replacement and WRITE_THROUGH waits for the move to reach disk.
    let result = unsafe {
        MoveFileExW(
            temporary_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn nul_terminated_windows_path(mut units: Vec<u16>) -> AppResult<Vec<u16>> {
    if units.contains(&0) {
        return Err(AppError::Message(
            "export destination contains an invalid NUL character".into(),
        ));
    }
    units.push(0);
    Ok(units)
}

#[cfg(not(windows))]
fn replace_destination(temporary: &Path, destination: &Path) -> AppResult<()> {
    // A sibling rename is atomic on the Unix filesystems supported by Tauri.
    fs::rename(temporary, destination)?;
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> AppResult<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> AppResult<()> {
    Ok(())
}

struct TemporaryCleanup {
    path: Option<PathBuf>,
}

impl TemporaryCleanup {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    fn disarm(&mut self) {
        self.path = None;
    }
}

impl Drop for TemporaryCleanup {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("soheidesk-atomic-{}", Uuid::new_v4()));
            fs::create_dir(&path).expect("create test directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn temporary_files(directory: &Path) -> Vec<PathBuf> {
        fs::read_dir(directory)
            .expect("read test directory")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".tmp"))
            })
            .collect()
    }

    #[test]
    fn writes_and_verifies_a_new_file() {
        let directory = TestDirectory::new();
        let destination = directory.path().join("report.md");

        write_bytes(&destination, b"complete report").expect("atomic write");

        assert_eq!(
            fs::read(&destination).expect("read report"),
            b"complete report"
        );
        assert!(temporary_files(directory.path()).is_empty());
    }

    #[test]
    fn validation_failure_preserves_existing_file() {
        let directory = TestDirectory::new();
        let destination = directory.path().join("template.json");
        fs::write(&destination, b"old template").expect("old template");

        let result = write_file(
            &destination,
            |mut file| {
                file.write_all(b"new template")?;
                Ok(file)
            },
            |_| Err(AppError::Message("invalid generated JSON".into())),
        );

        assert!(result.is_err());
        assert_eq!(
            fs::read(&destination).expect("read old template"),
            b"old template"
        );
        assert!(temporary_files(directory.path()).is_empty());
    }

    #[test]
    fn write_failure_preserves_existing_file() {
        let directory = TestDirectory::new();
        let destination = directory.path().join("report.md");
        fs::write(&destination, b"old report").expect("old report");

        let result = write_file(
            &destination,
            |mut file| {
                file.write_all(b"partial")?;
                Err(AppError::Message("simulated full disk".into()))
            },
            |_| Ok(()),
        );

        assert!(result.is_err());
        assert_eq!(
            fs::read(&destination).expect("read old report"),
            b"old report"
        );
        assert!(temporary_files(directory.path()).is_empty());
    }

    #[test]
    fn successful_write_replaces_existing_file() {
        let directory = TestDirectory::new();
        let destination = directory.path().join("report.md");
        fs::write(&destination, b"old report").expect("old report");

        write_bytes(&destination, b"new report").expect("replace report");

        assert_eq!(
            fs::read(&destination).expect("read new report"),
            b"new report"
        );
        assert!(temporary_files(directory.path()).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn replacement_preserves_existing_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = TestDirectory::new();
        let destination = directory.path().join("report.md");
        fs::write(&destination, b"old report").expect("old report");
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o640))
            .expect("set permissions");

        write_bytes(&destination, b"new report").expect("replace report");

        let mode = fs::metadata(&destination)
            .expect("report metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o640);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_replace_a_symlink() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new();
        let target = directory.path().join("target.md");
        let destination = directory.path().join("report.md");
        fs::write(&target, b"original target").expect("target");
        symlink(&target, &destination).expect("symlink");

        let result = write_bytes(&destination, b"replacement");

        assert!(result.is_err());
        assert_eq!(fs::read(&target).expect("read target"), b"original target");
        assert!(temporary_files(directory.path()).is_empty());
    }

    #[test]
    fn windows_path_encoding_rejects_an_interior_nul() {
        assert!(nul_terminated_windows_path(vec![b'a' as u16, 0, b'b' as u16]).is_err());
        assert_eq!(
            nul_terminated_windows_path(vec![b'a' as u16]).expect("valid path"),
            vec![b'a' as u16, 0]
        );
    }
}
