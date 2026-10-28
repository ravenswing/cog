//! Wikilinks between notes, resolved the way Obsidian resolves them.
//!
//! A link names its target by file name, as in `[[Note]]`, or by the end of
//! its path, as in `[[folder/Note]]`, ignoring case and any `.md`. Headings,
//! block references, and aliases after the name are ignored, so
//! `[[Note#Heading|shown text]]` links to `Note`. Embeds, `![[Note]]`, count
//! as links too.

use rayon::prelude::*;
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use crate::error::{IoResultExt, Result};
use crate::report::Report;
use crate::vault::{NotePath, Vault};

#[expect(
    clippy::expect_used,
    reason = "the pattern is a constant, and any use in the tests compiles it"
)]
static WIKILINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\[\]\n]+)\]\]").expect("valid regex"));

/// A note and the links in it, as written between the brackets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Linked {
    pub path: NotePath,
    pub links: Vec<String>,
}

/// Reads the links in every note in `vault`, in path order.
///
/// Notes that can't be read are collected in the report's errors. Fails
/// only if the vault can't be walked.
pub fn linked_notes(vault: &Vault, skip_dirs: &BTreeSet<String>) -> Result<Report<Linked>> {
    let paths: Vec<NotePath> = vault.notes(skip_dirs).collect::<Result<_>>()?;
    Ok(paths
        .into_par_iter()
        .map(|path| {
            let full = vault.resolve(&path);
            let content = fs::read_to_string(&full).ctx("read", &full)?;
            let links = WIKILINK
                .captures_iter(&content)
                .filter_map(|cap| cap.get(1))
                .map(|link| link.as_str().to_owned())
                .collect();
            Ok(Linked { path, links })
        })
        .collect())
}

/// What a link points to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'notes> {
    Note(&'notes NotePath),
    /// A heading or block in the same note, as in `[[#Heading]]`.
    SameNote,
    /// A file with an extension other than `.md`, such as an image. Only
    /// notes are walked, so whether it exists isn't known.
    Attachment,
    Missing,
}

/// Finds the note a link points to.
#[derive(Debug)]
pub struct Resolver<'notes> {
    /// Each note's lowercase path without `.md`.
    by_path: BTreeMap<String, &'notes NotePath>,
    /// Notes by lowercase file name without `.md`. Where names clash, the
    /// note nearest the vault root comes first.
    by_name: BTreeMap<String, Vec<&'notes NotePath>>,
}

impl<'notes> Resolver<'notes> {
    /// Indexes `notes`. Notes whose paths aren't UTF-8 can't be linked to,
    /// so are left out.
    pub fn new(notes: impl IntoIterator<Item = &'notes NotePath>) -> Self {
        let mut by_path = BTreeMap::new();
        let mut by_name: BTreeMap<String, Vec<&NotePath>> = BTreeMap::new();
        for note in notes {
            let Some(path) = note.as_path().to_str() else {
                continue;
            };
            let key = key(path);
            let name = key.rsplit('/').next().unwrap_or(&key).to_owned();
            by_name.entry(name).or_default().push(note);
            by_path.insert(key, note);
        }
        for notes in by_name.values_mut() {
            notes.sort_by_key(|note| note.as_path().components().count());
        }
        Self { by_path, by_name }
    }

    /// Resolves `link`, the text between the brackets of a wikilink.
    #[must_use]
    pub fn resolve(&self, link: &str) -> Target<'notes> {
        let end = link.find(['#', '|']).unwrap_or(link.len());
        let name = link.get(..end).unwrap_or(link).trim();
        if name.is_empty() {
            return Target::SameNote;
        }

        let wanted = key(name);
        let found = if wanted.contains('/') {
            let suffix = format!("/{wanted}");
            self.by_path.get(&wanted).copied().or_else(|| {
                self.by_path
                    .iter()
                    .find(|(path, _)| path.ends_with(&suffix))
                    .map(|(_, note)| *note)
            })
        } else {
            self.by_name
                .get(&wanted)
                .and_then(|notes| notes.first())
                .copied()
        };

        match found {
            Some(note) => Target::Note(note),
            None if Path::new(&wanted).extension().is_some() => Target::Attachment,
            None => Target::Missing,
        }
    }
}

/// A link's lookup key: lowercase, without any `.md` extension.
fn key(path: &str) -> String {
    let lower = path.to_lowercase();
    match lower.strip_suffix(".md") {
        Some(stem) => stem.to_owned(),
        None => lower,
    }
}

/// A link to a note that doesn't exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokenLink<'notes> {
    /// The note the link is in.
    pub note: &'notes NotePath,
    /// The text between the brackets.
    pub link: &'notes str,
}

/// Each link to a note that doesn't exist.
#[must_use]
pub fn broken_links<'notes>(
    notes: &'notes [Linked],
    resolver: &Resolver,
) -> Vec<BrokenLink<'notes>> {
    notes
        .iter()
        .flat_map(|note| {
            note.links.iter().map(move |link| BrokenLink {
                note: &note.path,
                link,
            })
        })
        .filter(|broken| resolver.resolve(broken.link) == Target::Missing)
        .collect()
}

/// Notes that no other note links to.
#[must_use]
pub fn orphans<'notes>(notes: &'notes [Linked], resolver: &Resolver) -> Vec<&'notes NotePath> {
    let mut linked: BTreeSet<&NotePath> = BTreeSet::new();
    for note in notes {
        for link in &note.links {
            if let Target::Note(target) = resolver.resolve(link)
                && *target != note.path
            {
                linked.insert(target);
            }
        }
    }
    notes
        .iter()
        .map(|note| &note.path)
        .filter(|path| !linked.contains(path))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(raw: &[&str]) -> Vec<NotePath> {
        raw.iter().map(NotePath::new).collect()
    }

    #[test]
    fn resolves_like_obsidian() {
        let notes = paths(&["Top.md", "a/Note.md", "a/b/Note.md", "c/Other.md"]);
        let resolver = Resolver::new(&notes);
        let note = |i: usize| Target::Note(&notes[i]);
        let cases = [
            ("Top", note(0)),
            ("top.md", note(0)),
            ("Note", note(1)),
            ("b/Note", note(2)),
            ("a/b/note", note(2)),
            ("Other#Heading|shown", note(3)),
            ("Other#^block", note(3)),
            ("#Heading", Target::SameNote),
            ("cover.png", Target::Attachment),
            ("Missing", Target::Missing),
            ("x/Other", Target::Missing),
        ];
        for (link, expected) in cases {
            assert_eq!(resolver.resolve(link), expected, "{link}");
        }
    }

    #[test]
    fn finds_broken_links_and_orphans() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, content: &str| {
            let path = dir.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        };
        write(
            "a.md",
            "See [[b]], ![[sub/c]], [[gone|alias]] and ![[img.png]]",
        );
        write("b.md", "Back to [[a#Top]] and [[b]] itself");
        write("sub/c.md", "---\nrelated: \"[[d]]\"\n---\n");
        write("d.md", "");
        write("lonely.md", "[[lonely]]");

        let vault = Vault::open(dir.path()).unwrap();
        let report = linked_notes(&vault, &BTreeSet::new()).unwrap();
        let resolver = Resolver::new(report.done.iter().map(|note| &note.path));

        let broken = broken_links(&report.done, &resolver);
        assert_eq!(
            broken,
            [BrokenLink {
                note: &NotePath::new("a.md"),
                link: "gone|alias",
            }]
        );
        assert_eq!(
            orphans(&report.done, &resolver),
            [&NotePath::new("lonely.md")]
        );
    }
}
