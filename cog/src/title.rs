//! Adding an H1 title to notes that lack one.

use rayon::prelude::*;
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::Path;

use crate::error::{Error, IoContext, Result};
use crate::fsutil::write_atomic;
use crate::note::{Split, split_frontmatter};
use crate::report::Report;
use crate::vault::{NotePath, Vault};

/// A title to add to one note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TitleEdit {
    pub path: NotePath,
    /// The note's full contents, with the title added.
    contents: String,
}

impl fmt::Display for Report<TitleEdit> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} titles added, {} errors",
            self.done.len(),
            self.errors.len()
        )
    }
}

/// Works out which notes in `vault` need a title, without changing any.
///
/// Excalidraw drawings, stored as `.excalidraw.md`, are left alone. Fails
/// only if the vault can't be walked.
pub fn plan_titles(vault: &Vault, skip_dirs: &BTreeSet<String>) -> Result<Report<TitleEdit>> {
    let paths: Vec<NotePath> = vault
        .notes(skip_dirs)
        .filter(|path| !path.as_ref().is_ok_and(is_drawing))
        .collect::<Result<_>>()?;
    Ok(paths
        .into_par_iter()
        .map(|path| {
            let full = vault.resolve(&path);
            let content = fs::read_to_string(&full).ctx("read", &full)?;
            Ok(with_title(&full, &content)?.map(|contents| TitleEdit { path, contents }))
        })
        .filter_map(Result::transpose)
        .collect())
}

/// Writes the titles in `plan`.
///
/// Edits that fail to write move from `done` to `errors`.
pub fn apply_titles(vault: &Vault, plan: Report<TitleEdit>) -> Report<TitleEdit> {
    let Report { done, mut errors } = plan;
    let applied: Report<TitleEdit> = done
        .into_par_iter()
        .map(|edit| {
            write_atomic(&vault.resolve(&edit.path), edit.contents.as_bytes())?;
            tracing::info!("added title: {}", edit.path);
            Ok(edit)
        })
        .collect();
    errors.extend(applied.errors);
    Report {
        done: applied.done,
        errors,
    }
}

/// Excalidraw drawings are stored as Markdown, but aren't notes to title.
fn is_drawing(path: &NotePath) -> bool {
    path.as_path()
        .file_name()
        .is_some_and(|name| name.as_encoded_bytes().ends_with(b".excalidraw.md"))
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

    // A lossy conversion would write U+FFFD into the note
    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_str()
        .ok_or_else(|| Error::NonUtf8Name {
            path: path.to_owned(),
        })?;
    let base = stem.split_once(" - ").map_or(stem, |(_, rest)| rest);
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
        let plan = plan_titles(&vault, &BTreeSet::new()).unwrap();

        let paths: Vec<_> = plan.done.iter().map(|edit| &edit.path).collect();
        assert_eq!(paths, [&NotePath::new("a.md")]);
        assert_eq!(
            fs::read_to_string(dir.path().join("b.excalidraw.md")).unwrap(),
            ""
        );
    }

    #[test]
    fn planning_writes_nothing_and_applying_writes_titles() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.md"), "body").unwrap();
        fs::write(dir.path().join("b.md"), "# Titled").unwrap();
        let vault = Vault::open(dir.path()).unwrap();

        let plan = plan_titles(&vault, &BTreeSet::new()).unwrap();
        assert_eq!(plan.done.len(), 1);
        assert_eq!(fs::read_to_string(dir.path().join("a.md")).unwrap(), "body");

        let applied = apply_titles(&vault, plan);
        assert_eq!(applied.to_string(), "1 titles added, 0 errors");
        assert_eq!(
            fs::read_to_string(dir.path().join("a.md")).unwrap(),
            "# A\nbody"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_names_that_are_not_utf8() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let path = Path::new(OsStr::from_bytes(b"bad\xffname.md"));
        assert!(matches!(
            with_title(path, "body"),
            Err(Error::NonUtf8Name { .. })
        ));
    }

    #[test]
    fn rejects_unclosed_frontmatter() {
        assert!(matches!(
            with_title(Path::new("n.md"), "---\na: 1\n"),
            Err(Error::UnclosedFrontmatter { .. })
        ));
    }
}
