//! File writes that never leave a half-written note behind.
//!
//! Each write goes to a hidden sibling file first and is then renamed into
//! place, which is atomic on the same filesystem.

use filetime::FileTime;
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;

use crate::error::{IoContext, Result};

/// Writes `contents` to `path`, keeping the permissions of any file it replaces.
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let mut tmp = temp_beside(path)?;
    tmp.write_all(contents).ctx("write", tmp.path())?;
    if let Ok(meta) = fs::metadata(path) {
        tmp.as_file()
            .set_permissions(meta.permissions())
            .ctx("set permissions on", tmp.path())?;
    }
    persist(tmp, path)
}

/// Copies `src` to `dest`, keeping the source's permissions and mtime.
///
/// Sync relies on copies keeping their mtime, otherwise every copied note
/// would look modified on the next run.
pub fn copy_preserving_mtime(src: &Path, dest: &Path) -> Result<()> {
    let meta = fs::metadata(src).ctx("read metadata of", src)?;
    let tmp = temp_beside(dest)?;
    fs::copy(src, tmp.path()).ctx("copy to", tmp.path())?;
    filetime::set_file_times(
        tmp.path(),
        FileTime::from_last_access_time(&meta),
        FileTime::from_last_modification_time(&meta),
    )
    .ctx("set times on", tmp.path())?;
    persist(tmp, dest)
}

/// Creates `path`'s parent directories and a hidden temp file beside it.
///
/// The temp file is deleted when dropped, so an error part way through a
/// write never leaves one behind.
fn temp_beside(path: &Path) -> Result<NamedTempFile> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent).ctx("create directory", parent)?;
    let prefix = format!(
        ".{}.",
        path.file_name().unwrap_or_default().to_string_lossy()
    );

    let mut builder = tempfile::Builder::new();
    builder.prefix(&prefix).suffix(".cog-tmp");
    // Leave the mode to the umask, as `fs::write` does, rather than 0600
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o666));
    }
    builder
        .tempfile_in(parent)
        .ctx("create temporary file in", parent)
}

fn persist(tmp: NamedTempFile, path: &Path) -> Result<()> {
    tmp.persist(path)
        .map(drop)
        .map_err(|e| e.error)
        .ctx("replace", path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    #[test]
    fn copy_keeps_mtime_and_creates_parents() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("a.md");
        let dest = dir.path().join("nested/deeper/a.md");
        fs::write(&src, "hello").unwrap();
        let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        filetime::set_file_mtime(&src, FileTime::from_system_time(old)).unwrap();

        copy_preserving_mtime(&src, &dest).unwrap();

        assert_eq!(fs::read_to_string(&dest).unwrap(), "hello");
        assert_eq!(fs::metadata(&dest).unwrap().modified().unwrap(), old);
    }

    #[test]
    fn leaves_no_temp_files_behind() {
        let dir = tempfile::tempdir().unwrap();
        write_atomic(&dir.path().join("a.md"), b"a").unwrap();
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["a.md"]);
    }

    #[cfg(unix)]
    #[test]
    fn new_files_get_the_same_mode_as_fs_write() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let mode = |name| {
            fs::metadata(dir.path().join(name))
                .unwrap()
                .permissions()
                .mode()
        };
        fs::write(dir.path().join("reference.md"), "").unwrap();
        write_atomic(&dir.path().join("a.md"), b"").unwrap();
        assert_eq!(mode("a.md"), mode("reference.md"));
    }

    #[test]
    fn write_replaces_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "old").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }
}
