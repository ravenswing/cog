use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::error::{Error, Result};

/// A note's path, relative to the root of its vault.
///
/// Only a [`Vault`] creates these, and [`Vault::resolve`] turns one back into
/// a full path, so a relative path can't be joined onto the wrong root by
/// accident.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NotePath(PathBuf);

impl NotePath {
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl fmt::Display for NotePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.display().fmt(f)
    }
}

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

    /// The full path of a note in this vault.
    pub fn resolve(&self, path: &NotePath) -> PathBuf {
        self.root.join(&path.0)
    }

    /// All notes in the vault, in path order.
    ///
    /// Directories named in `skip_dirs` are not entered, at any depth.
    pub fn notes<'a>(
        &'a self,
        skip_dirs: &'a BTreeSet<String>,
    ) -> impl Iterator<Item = Result<NotePath>> + 'a {
        WalkDir::new(&self.root)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(move |entry| {
                let skipped = entry.depth() > 0
                    && entry.file_type().is_dir()
                    && entry
                        .file_name()
                        .to_str()
                        .is_some_and(|d| skip_dirs.contains(d));
                !skipped
            })
            .filter_map(move |entry| match entry {
                Err(e) => Some(Err(e.into())),
                Ok(entry) if !entry.file_type().is_dir() && is_markdown(entry.path()) => {
                    let path = entry.path();
                    let relative = path.strip_prefix(&self.root).unwrap_or(path);
                    Some(Ok(NotePath::new(relative)))
                }
                Ok(_) => None,
            })
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
    fn walks_notes_in_order_skipping_dirs() {
        let dir = tempfile::tempdir().unwrap();
        for path in [
            "z.md",
            "sub/b.md",
            "sub/templates/c.md",
            "templates/d.md",
            "e.txt",
            "a.md",
        ] {
            let path = dir.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "").unwrap();
        }

        let vault = Vault::open(dir.path()).unwrap();
        let paths: Vec<_> = vault
            .notes(&BTreeSet::from(["templates".into()]))
            .collect::<Result<_>>()
            .unwrap();

        let expected = ["a.md", "sub/b.md", "z.md"].map(NotePath::new);
        assert_eq!(paths, expected);
        assert_eq!(vault.resolve(&paths[0]), dir.path().join("a.md"));
    }

    #[test]
    fn open_rejects_missing_dir() {
        assert!(matches!(
            Vault::open("/definitely/not/here"),
            Err(Error::NotADirectory { .. })
        ));
    }
}
