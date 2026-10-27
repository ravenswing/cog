use serde_norway::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::error::{Error, IoContext, Result};
use crate::tags::{Tag, extract_tags};

/// How many lines of the body are searched for an H1 title.
const TITLE_LINES: usize = 10;

/// A Markdown note, read-only.
///
/// The frontmatter is kept as raw text and never re-serialised, so comments
/// and formatting survive any edit made elsewhere.
#[derive(Debug, Clone)]
pub struct Note {
    pub path: PathBuf,
    /// The first H1 near the top of the body, else the file stem.
    pub title: String,
    /// The text between the `---` delimiters, if there are any.
    pub frontmatter: Option<String>,
    /// Everything after the frontmatter.
    pub body: String,
    pub tags: BTreeSet<Tag>,
    pub mtime: SystemTime,
}

impl Note {
    /// Reads and parses the note at `path`.
    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path).ctx("read", path)?;
        let mtime = fs::metadata(path)
            .and_then(|m| m.modified())
            .ctx("read mtime of", path)?;
        Self::parse(path, &content, mtime)
    }

    /// Builds a note from already-read contents.
    pub fn parse(path: &Path, content: &str, mtime: SystemTime) -> Result<Self> {
        let Split {
            frontmatter, body, ..
        } = split_frontmatter(content).ok_or_else(|| Error::UnclosedFrontmatter {
            path: path.to_owned(),
        })?;

        let yaml = frontmatter
            .map(serde_norway::from_str::<Value>)
            .transpose()
            .map_err(|source| Error::Yaml {
                path: path.to_owned(),
                source,
            })?;

        Ok(Note {
            path: path.to_owned(),
            title: find_title(body).unwrap_or_else(|| stem(path)),
            frontmatter: frontmatter.map(str::to_owned),
            body: body.to_owned(),
            tags: extract_tags(yaml.as_ref(), body),
            mtime,
        })
    }
}

/// A note's contents, split around its frontmatter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Split<'a> {
    /// Everything before the body, including both `---` lines. Empty when
    /// there is no frontmatter.
    pub header: &'a str,
    /// The text between the `---` lines, if there are any.
    pub frontmatter: Option<&'a str>,
    pub body: &'a str,
}

/// Splits a note around its frontmatter.
///
/// Frontmatter only counts when the very first line is `---`. Returns `None`
/// when that block is never closed.
pub fn split_frontmatter(content: &str) -> Option<Split<'_>> {
    let opening = content
        .split_inclusive('\n')
        .next()
        .filter(|line| line.trim() == "---");
    let Some(opening) = opening else {
        return Some(Split {
            header: "",
            frontmatter: None,
            body: content,
        });
    };

    let rest = content.strip_prefix(opening)?;
    let mut frontmatter_len = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim() == "---" {
            let (frontmatter, closing) = rest.split_at_checked(frontmatter_len)?;
            let body = closing.strip_prefix(line)?;
            return Some(Split {
                header: content.strip_suffix(body)?,
                frontmatter: Some(frontmatter),
                body,
            });
        }
        frontmatter_len += line.len();
    }
    None
}

fn find_title(body: &str) -> Option<String> {
    body.lines()
        .take(TITLE_LINES)
        .find_map(|line| line.strip_prefix("# "))
        .map(|title| title.trim().to_string())
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    fn parse(content: &str) -> Result<Note> {
        Note::parse(Path::new("dir/My Note.md"), content, UNIX_EPOCH)
    }

    #[test]
    fn splits_frontmatter() {
        let split = |content| split_frontmatter(content).map(|s| (s.frontmatter, s.body));
        let cases = [
            ("---\na: 1\n---\nbody", Some((Some("a: 1\n"), "body"))),
            (
                "---\r\na: 1\r\n---\r\nbody",
                Some((Some("a: 1\r\n"), "body")),
            ),
            ("---\n---\nbody", Some((Some(""), "body"))),
            (
                "no frontmatter\n---\n",
                Some((None, "no frontmatter\n---\n")),
            ),
            ("", Some((None, ""))),
            ("---\na: 1\nnever closed", None),
        ];
        for (content, expected) in cases {
            assert_eq!(split(content), expected, "{content:?}");
        }
    }

    #[test]
    fn header_is_everything_before_body() {
        let content = "---\r\na: 1\r\n---\r\nbody";
        let split = split_frontmatter(content).unwrap();
        assert_eq!(split.header, "---\r\na: 1\r\n---\r\n");
        assert_eq!(format!("{}{}", split.header, split.body), content);
        assert_eq!(split_frontmatter("body").unwrap().header, "");
    }

    #[test]
    fn title_from_h1_or_stem() {
        assert_eq!(
            parse("---\na: 1\n---\n# Real Title\n").unwrap().title,
            "Real Title"
        );
        assert_eq!(parse("## Sub\ntext # not\n").unwrap().title, "My Note");
        assert_eq!(parse("").unwrap().title, "My Note");
    }

    #[test]
    fn collects_tags() {
        let note = parse("---\ntags: [a]\n---\n#b\n").unwrap();
        let tags: Vec<&str> = note.tags.iter().map(Tag::as_str).collect();
        assert_eq!(tags, ["a", "b"]);
    }

    #[test]
    fn rejects_bad_frontmatter() {
        assert!(matches!(
            parse("---\nunclosed"),
            Err(Error::UnclosedFrontmatter { .. })
        ));
        assert!(matches!(parse("---\na: [\n---\n"), Err(Error::Yaml { .. })));
    }
}
