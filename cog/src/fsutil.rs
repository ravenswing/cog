//! File writes that never leave a half-written note behind.
//!
//! Each write goes to a hidden sibling file first and is then renamed into
//! place, which is atomic on the same filesystem.

use filetime::FileTime;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{IoContext, Result};

/// Writes `contents` to `path`, keeping the permissions of any file it replaces.
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let tmp = prepare_tmp(path)?;
    fs::write(&tmp, contents).ctx("write", &tmp)?;
    if let Ok(meta) = fs::metadata(path) {
        fs::set_permissions(&tmp, meta.permissions()).ctx("set permissions on", &tmp)?;
    }
    finish(&tmp, path)
}

/// Copies `src` to `dest`, keeping the source's permissions and mtime.
///
/// Sync relies on copies keeping their mtime, otherwise every copied note
/// would look modified on the next run.
pub fn copy_preserving_mtime(src: &Path, dest: &Path) -> Result<()> {
    let meta = fs::metadata(src).ctx("read metadata of", src)?;
    let tmp = prepare_tmp(dest)?;
    fs::copy(src, &tmp).ctx("copy to", &tmp)?;
    filetime::set_file_times(
        &tmp,
        FileTime::from_last_access_time(&meta),
        FileTime::from_last_modification_time(&meta),
    )
    .ctx("set times on", &tmp)?;
    finish(&tmp, dest)
}

/// Creates `path`'s parent directories and returns a temp path beside it.
fn prepare_tmp(path: &Path) -> Result<PathBuf> {
    let parent = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent).ctx("create directory", parent)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    Ok(parent.join(format!(".{name}.cog-tmp")))
}

fn finish(tmp: &Path, path: &Path) -> Result<()> {
    fs::rename(tmp, path).or_else(|e| {
        // Best effort, the rename failure is the error worth reporting
        if let Err(cleanup) = fs::remove_file(tmp) {
            tracing::debug!("failed to remove {}: {cleanup}", tmp.display());
        }
        Err(e).ctx("replace", path)
    })
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
        assert!(!dest.with_file_name(".a.md.cog-tmp").exists());
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
