//! Tag extraction following Obsidian's rules.
//!
//! Tags are lowercased, since Obsidian treats `#Game` and `#game` as one
//! tag, and nested tags also yield their parents: `games/rpg` adds `games`
//! alongside `games/rpg`, as searching Obsidian for `#games` finds both. The
//! last part, `rpg`, is not a tag on its own.

use regex::Regex;
use serde::Deserialize;
use serde_norway::Value;
use std::borrow::Borrow;
use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;
use std::sync::LazyLock;

/// How many lines of the body are searched for inline tags.
const INLINE_TAG_LINES: usize = 15;

#[expect(
    clippy::expect_used,
    reason = "the pattern is a constant, and any use in the tests compiles it"
)]
static WIKILINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)\[\[.*?\]\]").expect("valid regex"));
#[expect(
    clippy::expect_used,
    reason = "the pattern is a constant, and any use in the tests compiles it"
)]
static INLINE_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|\s)#[\p{L}\p{N}_/-]+").expect("valid regex"));

/// A tag, normalised the way Obsidian compares them.
///
/// Always trimmed, lowercase, and without a leading `#`, with at least one
/// character that isn't a digit or `/`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct Tag(String);

/// Text that can't be read as a tag, such as an empty or all-numeric one.
#[derive(Debug, thiserror::Error)]
#[error("{0:?} is not a valid tag")]
pub struct InvalidTag(String);

impl Tag {
    /// Normalises `raw`, or returns `None` if it isn't a valid tag.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let tag = raw.trim().trim_start_matches('#').to_lowercase();
        // Obsidian requires at least one non-numeric character
        let numeric = tag.chars().all(|c| c.is_numeric() || c == '/');
        (!numeric).then_some(Self(tag))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The tag itself, followed by each parent if it is nested.
    ///
    /// `games/2024/rpg` yields `games` and `games/2024` as well. Parents that
    /// aren't valid tags on their own, such as `2024` in `2024/q1`, are left
    /// out.
    pub fn with_parents(self) -> impl Iterator<Item = Tag> {
        let parents: Vec<Tag> = self
            .0
            .match_indices('/')
            .filter_map(|(end, _)| self.0.get(..end))
            .filter_map(Tag::parse)
            .collect();
        std::iter::once(self).chain(parents)
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Tag {
    type Err = InvalidTag;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| InvalidTag(s.to_owned()))
    }
}

impl TryFrom<String> for Tag {
    type Error = InvalidTag;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl Borrow<str> for Tag {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// Collects the tags from a note's parsed frontmatter and the top of its body.
pub fn extract_tags(frontmatter: Option<&Value>, body: &str) -> BTreeSet<Tag> {
    frontmatter
        .map(frontmatter_tags)
        .unwrap_or_default()
        .into_iter()
        .filter_map(Tag::parse)
        .chain(inline_tags(body))
        .flat_map(Tag::with_parents)
        .collect()
}

/// Tags under the `tags` or `tag` keys, as a list or comma-separated string.
fn frontmatter_tags(frontmatter: &Value) -> Vec<&str> {
    ["tags", "tag"]
        .into_iter()
        .filter_map(|key| frontmatter.get(key))
        .flat_map(|value| match value {
            Value::String(s) => s.split(',').collect(),
            Value::Sequence(seq) => seq.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        })
        .collect()
}

/// `#tags` in the first lines of the body, ignoring any inside `[[links]]`.
fn inline_tags(body: &str) -> Vec<Tag> {
    let head_len = body
        .split_inclusive('\n')
        .take(INLINE_TAG_LINES)
        .map(str::len)
        .sum();
    let head = body.get(..head_len).unwrap_or(body);
    // Only allocates if there are links to remove
    let text = WIKILINK.replace_all(head, "");
    // Matches start with whitespace and `#`, which `Tag::parse` strips.
    // Without a capture group the regex only has to find each match.
    INLINE_TAG
        .find_iter(&text)
        .filter_map(|m| Tag::parse(m.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(frontmatter: &str, body: &str) -> Vec<String> {
        let value: Value = serde_norway::from_str(frontmatter).unwrap();
        extract_tags(Some(&value), body)
            .into_iter()
            .map(|tag| tag.to_string())
            .collect()
    }

    #[test]
    fn parses_and_normalises() {
        let cases = [
            (" #Game ", Some("game")),
            ("no-sync", Some("no-sync")),
            ("2024", None),
            ("2024/01", None),
            ("#", None),
            ("", None),
        ];
        for (raw, expected) in cases {
            assert_eq!(
                Tag::parse(raw).as_ref().map(Tag::as_str),
                expected,
                "{raw:?}"
            );
        }
    }

    #[test]
    fn nested_tags_yield_their_parents() {
        let parents = |raw| -> Vec<String> {
            Tag::parse(raw)
                .unwrap()
                .with_parents()
                .map(|tag| tag.to_string())
                .collect()
        };
        assert_eq!(
            parents("games/2024/rpg"),
            ["games/2024/rpg", "games", "games/2024"]
        );
        assert_eq!(parents("2024/q1"), ["2024/q1"]);
        assert_eq!(parents("game"), ["game"]);
    }

    #[test]
    fn frontmatter_forms() {
        let cases = [
            ("tags: [game, rpg]", vec!["game", "rpg"]),
            ("tags:\n  - game\n  - rpg", vec!["game", "rpg"]),
            ("tags: game, rpg", vec!["game", "rpg"]),
            ("tag: game", vec!["game"]),
            ("tags: '#game'", vec!["game"]),
            ("tags: [Game]", vec!["game"]),
            ("tags:", vec![]),
            ("tags: 5", vec![]),
            ("title: nothing", vec![]),
        ];
        for (frontmatter, expected) in cases {
            assert_eq!(tags(frontmatter, ""), expected, "{frontmatter}");
        }
    }

    #[test]
    fn inline_forms() {
        let cases = [
            ("#no-sync", vec!["no-sync"]),
            ("text #Game2 more", vec!["game2"]),
            ("#tag, then text", vec!["tag"]),
            ("#games/rpg", vec!["games", "games/rpg"]),
            ("#café", vec!["café"]),
            ("# Heading", vec![]),
            ("## Heading", vec![]),
            ("#2024", vec![]),
            ("url.com/page#anchor", vec![]),
            ("see [[note#section]]", vec![]),
        ];
        for (body, expected) in cases {
            assert_eq!(tags("{}", body), expected, "{body}");
        }
    }

    #[test]
    fn inline_tags_only_in_first_lines() {
        let body = format!("{}#late", "line\n".repeat(INLINE_TAG_LINES));
        assert!(tags("{}", &body).is_empty());
        let body = format!("{}#early", "line\n".repeat(INLINE_TAG_LINES - 1));
        assert_eq!(tags("{}", &body), ["early"]);
    }

    #[test]
    fn combines_frontmatter_and_inline() {
        assert_eq!(tags("tags: [a]", "#b #a"), ["a", "b"]);
    }
}
