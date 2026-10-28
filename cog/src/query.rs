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

/// Whether a note needs every tag in a [`TagQuery`], or just one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Match {
    All,
    Any,
}

/// A search for notes by tag.
///
/// Nested tags match their parents, so `games` finds `games/rpg`, as in
/// Obsidian.
#[derive(Debug, Clone)]
pub struct TagQuery {
    pub tags: Vec<Tag>,
    pub mode: Match,
    /// Tags that rule a note out, whatever else it has.
    pub without: Vec<Tag>,
}

impl TagQuery {
    #[must_use]
    pub fn matches(&self, tags: &BTreeSet<Tag>) -> bool {
        let wanted = match self.mode {
            Match::All => self.tags.iter().all(|tag| tags.contains(tag)),
            Match::Any => self.tags.iter().any(|tag| tags.contains(tag)),
        };
        wanted && !self.without.iter().any(|tag| tags.contains(tag))
    }
}

/// How many notes have each tag.
#[must_use]
pub fn count_tags(notes: &[Tagged]) -> BTreeMap<&Tag, usize> {
    let mut counts = BTreeMap::new();
    for note in notes {
        for tag in &note.tags {
            let count: &mut usize = counts.entry(tag).or_default();
            *count = count.saturating_add(1);
        }
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
    fn queries_match_all_any_and_without() {
        let note: BTreeSet<Tag> = ["game", "games/rpg", "games"].map(tag).into();
        let query = |tags: &[&str], mode, without: &[&str]| TagQuery {
            tags: tags.iter().map(|t| tag(t)).collect(),
            mode,
            without: without.iter().map(|t| tag(t)).collect(),
        };
        let cases = [
            (query(&["game", "games"], Match::All, &[]), true),
            (query(&["game", "film"], Match::All, &[]), false),
            (query(&["game", "film"], Match::Any, &[]), true),
            (query(&["film"], Match::Any, &[]), false),
            (query(&["game"], Match::All, &["games"]), false),
            (query(&["game"], Match::All, &["film"]), true),
        ];
        for (query, expected) in cases {
            assert_eq!(query.matches(&note), expected, "{query:?}");
        }
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
