//! Tag extraction following Obsidian's rules.
//!
//! Tags are lowercased, since Obsidian treats `#Game` and `#game` as one
//! tag, and nested tags also yield their parts: `games/rpg` adds `games` and
//! `rpg` alongside `games/rpg`.

use regex::Regex;
use serde_norway::Value;
use std::collections::BTreeSet;
use std::sync::LazyLock;

/// How many lines of the body are searched for inline tags.
const INLINE_TAG_LINES: usize = 15;

static WIKILINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)\[\[.*?\]\]").expect("valid regex"));
static INLINE_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|\s)#([\p{L}\p{N}_/-]+)").expect("valid regex"));

/// Collects the tags from a note's parsed frontmatter and the top of its body.
pub fn extract_tags(frontmatter: Option<&Value>, body: &str) -> BTreeSet<String> {
    let mut tags = BTreeSet::new();
    if let Some(frontmatter) = frontmatter {
        for raw in frontmatter_tags(frontmatter) {
            insert(&mut tags, raw);
        }
    }
    for raw in inline_tags(body) {
        insert(&mut tags, raw);
    }
    tags
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
fn inline_tags(body: &str) -> Vec<String> {
    let head: Vec<&str> = body.lines().take(INLINE_TAG_LINES).collect();
    let text = WIKILINK.replace_all(&head.join("\n"), "").into_owned();
    INLINE_TAG
        .captures_iter(&text)
        .map(|cap| cap[1].to_string())
        .collect()
}

fn insert(tags: &mut BTreeSet<String>, raw: impl AsRef<str>) {
    let tag = raw.as_ref().trim().trim_start_matches('#').to_lowercase();
    // Obsidian requires at least one non-numeric character
    if tag.is_empty() || tag.chars().all(|c| c.is_numeric() || c == '/') {
        return;
    }
    if tag.contains('/') {
        for part in tag.split('/').filter(|p| !p.is_empty()) {
            tags.insert(part.to_string());
        }
    }
    tags.insert(tag);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(frontmatter: &str, body: &str) -> Vec<String> {
        let value: Value = serde_norway::from_str(frontmatter).unwrap();
        extract_tags(Some(&value), body).into_iter().collect()
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
            ("#games/rpg", vec!["games", "games/rpg", "rpg"]),
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
