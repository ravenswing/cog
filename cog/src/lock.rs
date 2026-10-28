//! A lock that stops two cog runs changing the vaults at the same time.

use std::fs::{self, File, TryLockError};
use std::path::{Path, PathBuf};

use crate::error::{Error, IoResultExt, Result};

/// Holds the run lock until dropped.
///
/// The operating system releases the lock when the file is closed, so a run
/// that crashes never leaves it held.
#[derive(Debug)]
pub struct RunLock {
    _file: File,
}

impl RunLock {
    /// Takes the lock kept beside the state file at `state_path`, failing at
    /// once if another run holds it.
    pub fn acquire(state_path: &Path) -> Result<Self> {
        let path = lock_path(state_path);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).ctx("create directory", dir)?;
        }
        let file = File::options()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .ctx("open lock file", &path)?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(Error::Locked { path }),
            Err(TryLockError::Error(e)) => Err(e).ctx("lock", &path),
        }
    }
}

/// The lock file beside the state file, so runs with separate state files
/// don't block each other.
fn lock_path(state_path: &Path) -> PathBuf {
    state_path.with_extension("lock")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_run_is_refused_until_the_first_ends() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("cog/state.toml");

        let first = RunLock::acquire(&state).unwrap();
        assert!(matches!(
            RunLock::acquire(&state),
            Err(Error::Locked { .. })
        ));
        drop(first);
        RunLock::acquire(&state).unwrap();
    }
}
