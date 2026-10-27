//! Questions about a whole vault, answered from every note's tags.

use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

use crate::error::Result;
use crate::note::load_tags;
use crate::report::Report;
use crate::tags::Tag;
use crate::vault::{NotePath, Vault};

/// A note and its tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tagged {
    pub path: NotePath,
    pub tags: BTreeSet<Tag>,
}

/// Reads the tags of every note in `vault`, in path order.
///
/// Notes that can't be read are collected in the report's errors. Fails
/// only if the vault can't be walked.
pub fn tagged_notes(vault: &Vault, skip_dirs: &BTreeSet<String>) -> Result<Report<Tagged>> {
    let paths: Vec<NotePath> = vault.notes(skip_dirs).collect::<Result<_>>()?;
    Ok(paths
        .into_par_iter()
        .map(|path| {
            let tags = load_tags(&vault.resolve(&path))?;
            Ok(Tagged { path, tags })
        })
        .collect())
}

/// How many notes have each tag.
pub fn count_tags(notes: &[Tagged]) -> BTreeMap<&Tag, usize> {
    let mut counts = BTreeMap::new();
    for tag in notes.iter().flat_map(|note| &note.tags) {
        let count: &mut usize = counts.entry(tag).or_default();
        *count = count.saturating_add(1);
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tag(raw: &str) -> Tag {
        Tag::parse(raw).unwrap()
    }

    #[test]
    fn counts_notes_per_tag_including_parents() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("a.md"),
            "---\ntags: [game]\n---\n#games/rpg",
        )
        .unwrap();
        fs::write(dir.path().join("b.md"), "#game #game").unwrap();
        fs::write(dir.path().join("bad.md"), "---\nunclosed").unwrap();

        let vault = Vault::open(dir.path()).unwrap();
        let report = tagged_notes(&vault, &BTreeSet::new()).unwrap();
        assert_eq!(report.errors.len(), 1);

        let counts = count_tags(&report.done);
        let expected = [("game", 2), ("games", 1), ("games/rpg", 1)];
        let expected: Vec<(Tag, usize)> = expected.map(|(t, n)| (tag(t), n)).into();
        let counts: Vec<(Tag, usize)> = counts.into_iter().map(|(t, n)| (t.clone(), n)).collect();
        assert_eq!(counts, expected);
    }
}
