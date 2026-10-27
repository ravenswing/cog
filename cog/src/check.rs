//! Finding notes that Obsidian can't fully read.
//!
//! Obsidian quietly ignores frontmatter it can't parse, so a note with a typo
//! in its YAML loses its properties and tags without any warning.

use rayon::prelude::*;
use std::collections::BTreeSet;
use std::fmt;

use crate::error::{Error, Result};
use crate::note::load_tags;
use crate::report::Report;
use crate::vault::{NotePath, Vault};

impl fmt::Display for Report<NotePath> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} notes checked, {} problems",
            self.done.len().saturating_add(self.errors.len()),
            self.errors.len()
        )
    }
}

/// Checks every note in `vault`, collecting the ones that pass in `done` and
/// a problem for each one that doesn't.
///
/// A note fails if it can't be read as UTF-8, its name isn't UTF-8, or its
/// frontmatter is unclosed or isn't valid YAML. Fails only if the vault
/// can't be walked.
pub fn check_vault(vault: &Vault, skip_dirs: &BTreeSet<String>) -> Result<Report<NotePath>> {
    let paths: Vec<NotePath> = vault.notes(skip_dirs).collect::<Result<_>>()?;
    Ok(paths
        .into_par_iter()
        .map(|path| check_note(vault, path))
        .collect())
}

fn check_note(vault: &Vault, path: NotePath) -> Result<NotePath> {
    let full = vault.resolve(&path);
    if path.as_path().to_str().is_none() {
        return Err(Error::NonUtf8Name { path: full });
    }
    load_tags(&full)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn reports_each_broken_note() {
        let dir = tempfile::tempdir().unwrap();
        for (name, content) in [
            ("good.md", "---\ntags: [a]\n---\nbody"),
            ("plain.md", "no frontmatter"),
            ("unclosed.md", "---\ntags: [a]\nbody"),
            ("bad-yaml.md", "---\ntags: [\n---\n"),
        ] {
            fs::write(dir.path().join(name), content).unwrap();
        }
        fs::write(dir.path().join("binary.md"), b"\xff\xfe").unwrap();

        let vault = Vault::open(dir.path()).unwrap();
        let report = check_vault(&vault, &BTreeSet::new()).unwrap();

        assert_eq!(
            report.done,
            [NotePath::new("good.md"), NotePath::new("plain.md")]
        );
        assert_eq!(report.to_string(), "5 notes checked, 3 problems");
        assert!(matches!(report.errors[0], Error::Yaml { .. }));
        assert!(matches!(report.errors[1], Error::Io { .. }));
        assert!(matches!(
            report.errors[2],
            Error::UnclosedFrontmatter { .. }
        ));
    }
}
