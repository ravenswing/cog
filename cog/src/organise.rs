//! Moving tagged notes into their own directory.

use std::fs;
use std::path::PathBuf;

use crate::error::{Error, IoContext, Result};
use crate::note::Note;
use crate::vault::Vault;

/// The outcome of organising a vault.
#[derive(Debug, Default)]
pub struct OrganiseReport {
    /// File names of the notes moved into the directory.
    pub moved: Vec<PathBuf>,
    /// File names left in place because the directory already has a note
    /// with the same name.
    pub clashes: Vec<PathBuf>,
    pub errors: Vec<Error>,
}

/// Moves notes in the root of `vault` tagged `tag` into a subdirectory.
///
/// The directory is named `dir_name`, or `tag` if not given, and is created
/// if needed. Only notes directly in the vault root are considered. With
/// `dry_run`, nothing is created or moved.
pub fn organise_by_tag(
    vault: &Vault,
    tag: &str,
    dir_name: Option<&str>,
    dry_run: bool,
) -> Result<OrganiseReport> {
    let tag = tag.trim_start_matches('#').to_lowercase();
    let tag_dir = vault.root().join(dir_name.unwrap_or(&tag));
    let mut report = OrganiseReport::default();

    let mut entries: Vec<_> = fs::read_dir(vault.root())
        .ctx("read directory", vault.root())?
        .map(|entry| entry.map(|e| e.path()))
        .collect::<std::io::Result<_>>()
        .ctx("read directory", vault.root())?;
    entries.sort();

    for path in entries {
        if !path.is_file() || path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        match Note::load(&path) {
            Ok(note) if note.tags.contains(&tag) => {}
            Ok(_) => continue,
            Err(e) => {
                report.errors.push(e);
                continue;
            }
        }

        let name = PathBuf::from(path.file_name().unwrap_or_default());
        let target = tag_dir.join(&name);
        if target.exists() {
            report.clashes.push(name);
            continue;
        }
        if !dry_run {
            fs::create_dir_all(&tag_dir).ctx("create directory", &tag_dir)?;
            if let Err(e) = fs::rename(&path, &target).ctx("move", &path) {
                report.errors.push(e);
                continue;
            }
        }
        report.moved.push(name);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_tagged_root_notes_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("games")).unwrap();
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("a.md"), "#game").unwrap();
        fs::write(root.join("b.md"), "---\ntags: [Game]\n---\n").unwrap();
        fs::write(root.join("c.md"), "#game").unwrap();
        fs::write(root.join("games/c.md"), "existing").unwrap();
        fs::write(root.join("d.md"), "untagged").unwrap();
        fs::write(root.join("sub/e.md"), "#game").unwrap();

        let vault = Vault::open(root).unwrap();
        let report = organise_by_tag(&vault, "game", Some("games"), false).unwrap();

        assert_eq!(report.moved, [PathBuf::from("a.md"), PathBuf::from("b.md")]);
        assert_eq!(report.clashes, [PathBuf::from("c.md")]);
        assert!(root.join("games/a.md").exists());
        assert_eq!(
            fs::read_to_string(root.join("games/c.md")).unwrap(),
            "existing"
        );
        assert!(root.join("c.md").exists());
        assert!(root.join("d.md").exists());
        assert!(root.join("sub/e.md").exists());
    }

    #[test]
    fn dry_run_moves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.md"), "#game").unwrap();

        let vault = Vault::open(dir.path()).unwrap();
        let report = organise_by_tag(&vault, "game", None, true).unwrap();

        assert_eq!(report.moved, [PathBuf::from("a.md")]);
        assert!(dir.path().join("a.md").exists());
        assert!(!dir.path().join("game").exists());
    }
}
