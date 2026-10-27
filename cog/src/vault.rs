use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::error::{Error, Result};

/// An Obsidian vault on disk.
#[derive(Debug, Clone)]
pub struct Vault {
    root: PathBuf,
}

impl Vault {
    /// Opens the vault at `root`, which must be an existing directory.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        if !root.is_dir() {
            return Err(Error::NotADirectory { path: root });
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Paths of all notes, relative to the root.
    ///
    /// Directories named in `skip_dirs` are not entered, at any depth.
    pub fn note_paths(&self, skip_dirs: &[String]) -> Result<BTreeSet<PathBuf>> {
        let walker = WalkDir::new(&self.root).into_iter().filter_entry(|entry| {
            let skipped = entry.depth() > 0
                && entry.file_type().is_dir()
                && skip_dirs.iter().any(|d| entry.file_name() == d.as_str());
            !skipped
        });

        let mut paths = BTreeSet::new();
        for entry in walker {
            let entry = entry?;
            if !entry.file_type().is_dir() && is_markdown(entry.path()) {
                let relative = entry
                    .path()
                    .strip_prefix(&self.root)
                    .unwrap_or(entry.path());
                paths.insert(relative.to_owned());
            }
        }
        Ok(paths)
    }
}

fn is_markdown(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "md")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn walks_notes_skipping_dirs() {
        let dir = tempfile::tempdir().unwrap();
        for path in [
            "a.md",
            "sub/b.md",
            "sub/templates/c.md",
            "templates/d.md",
            "e.txt",
        ] {
            let path = dir.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "").unwrap();
        }

        let vault = Vault::open(dir.path()).unwrap();
        let paths = vault.note_paths(&["templates".into()]).unwrap();

        let expected: BTreeSet<PathBuf> = ["a.md", "sub/b.md"].map(PathBuf::from).into();
        assert_eq!(paths, expected);
    }

    #[test]
    fn open_rejects_missing_dir() {
        assert!(matches!(
            Vault::open("/definitely/not/here"),
            Err(Error::NotADirectory { .. })
        ));
    }
}
