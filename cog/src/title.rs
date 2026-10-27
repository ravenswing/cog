//! Adding an H1 title to notes that lack one.

use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, IoContext, Result};
use crate::fsutil::write_atomic;
use crate::note::{Split, split_frontmatter};
use crate::vault::Vault;

/// The outcome of titling a whole vault.
#[derive(Debug, Default)]
pub struct TitleReport {
    /// Notes that were given a title, relative to the vault root.
    pub added: Vec<PathBuf>,
    pub errors: Vec<Error>,
}

/// Titles every untitled note in `vault`, or only reports them if `dry_run`.
///
/// Excalidraw drawings, stored as `.excalidraw.md`, are left alone.
pub fn add_titles(vault: &Vault, skip_dirs: &[String], dry_run: bool) -> Result<TitleReport> {
    let paths: Vec<PathBuf> = vault
        .note_paths(skip_dirs)?
        .into_iter()
        .filter(|path| !path.to_string_lossy().ends_with(".excalidraw.md"))
        .collect();
    let results: Vec<_> = paths
        .into_par_iter()
        .map(|path| {
            let full = vault.root().join(&path);
            let content = fs::read_to_string(&full).ctx("read", &full)?;
            let Some(titled) = with_title(&full, &content)? else {
                return Ok(None);
            };
            if !dry_run {
                write_atomic(&full, titled.as_bytes())?;
            }
            Ok(Some(path))
        })
        .collect();

    let mut report = TitleReport::default();
    for result in results {
        match result {
            Ok(Some(path)) => report.added.push(path),
            Ok(None) => {}
            Err(e) => report.errors.push(e),
        }
    }
    Ok(report)
}

/// Returns `content` with a title inserted after the frontmatter, or `None`
/// if the body already starts with an H1.
///
/// The title is the file stem without any `prefix - ` annotation, with its
/// first letter capitalised.
pub fn with_title(path: &Path, content: &str) -> Result<Option<String>> {
    let Split { header, body, .. } =
        split_frontmatter(content).ok_or_else(|| Error::UnclosedFrontmatter {
            path: path.to_owned(),
        })?;

    let first_line = body.lines().find(|line| !line.trim().is_empty());
    if first_line.is_some_and(|line| line.starts_with("# ")) {
        return Ok(None);
    }

    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let base = stem.split_once(" - ").map_or(&*stem, |(_, rest)| rest);
    let mut chars = base.chars();
    let title: String = chars
        .next()
        .into_iter()
        .flat_map(char::to_uppercase)
        .chain(chars)
        .collect();

    Ok(Some(format!("{header}# {title}\n{body}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titled(name: &str, content: &str) -> Option<String> {
        with_title(Path::new(name), content).unwrap()
    }

    #[test]
    fn inserts_after_frontmatter() {
        assert_eq!(
            titled("note.md", "---\na: 1\n---\nbody\n").unwrap(),
            "---\na: 1\n---\n# Note\nbody\n"
        );
    }

    #[test]
    fn inserts_at_top_without_frontmatter() {
        assert_eq!(titled("note.md", "body").unwrap(), "# Note\nbody");
        assert_eq!(titled("note.md", "").unwrap(), "# Note\n");
    }

    #[test]
    fn strips_prefix_and_capitalises_first_letter_only() {
        assert_eq!(titled("python - uv.md", "").unwrap(), "# Uv\n");
        assert_eq!(
            titled("nbd - MCS-PELE notes.md", "").unwrap(),
            "# MCS-PELE notes\n"
        );
        assert_eq!(titled("a - b - c.md", "").unwrap(), "# B - c\n");
    }

    #[test]
    fn leaves_titled_notes_alone() {
        assert_eq!(titled("note.md", "# Existing\nbody"), None);
        assert_eq!(titled("note.md", "---\na: 1\n---\n\n# Existing\n"), None);
    }

    #[test]
    fn keeps_line_endings() {
        assert_eq!(
            titled("note.md", "---\r\na: 1\r\n---\r\nbody\r\n").unwrap(),
            "---\r\na: 1\r\n---\r\n# Note\nbody\r\n"
        );
    }

    #[test]
    fn skips_excalidraw_drawings() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.md"), "").unwrap();
        fs::write(dir.path().join("b.excalidraw.md"), "").unwrap();

        let vault = Vault::open(dir.path()).unwrap();
        let report = add_titles(&vault, &[], false).unwrap();

        assert_eq!(report.added, [PathBuf::from("a.md")]);
        assert_eq!(
            fs::read_to_string(dir.path().join("b.excalidraw.md")).unwrap(),
            ""
        );
    }

    #[test]
    fn rejects_unclosed_frontmatter() {
        assert!(matches!(
            with_title(Path::new("n.md"), "---\na: 1\n"),
            Err(Error::UnclosedFrontmatter { .. })
        ));
    }
}
